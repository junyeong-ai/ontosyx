//! Data-driven SHACL enforcement against Cypher writes.
//!
//! `OntologyIR::rules` carries SHACL-Core `RuleDef`s authored by the
//! ontology team. Until now they were pure data — no runtime consumer
//! read them, so a freshly added rule made no difference until the
//! next code release. This validator is the bridge: it reads
//! `ontology.rules()`, filters to the pre-execute surface
//! (`EnforcementKind::Write` + `RuleActivationKind::Always`), and
//! emits [`ValidationIssue`]s when a CREATE / MERGE pattern violates
//! the declared constraints.
//!
//! ## Scope
//!
//! The validator covers every write surface the AST exposes — every
//! point where a property's value lands or leaves the graph:
//!
//! - **`CREATE` / `MERGE`** on a `NodePattern` runs the full property
//!   battery against the inline map: `MinCount { min >= 1 }`,
//!   `InValueSet`, `MatchesPattern` (notation pattern), `LessThan` /
//!   `Equals` property-pair predicates, plus the node-level
//!   `Closed` / `Disjoint` / `UniqueKey` / `UniqueLang` checks.
//! - **`SET n.prop = value`** resolves `n` to its labels via the
//!   statement's MATCH / CREATE / MERGE patterns, then runs the
//!   `InValueSet` and `MatchesPattern` checks against the new value
//!   the same way as the inline-map path. The synthetic single-key
//!   `NodePattern` keeps `check_property_constraint` as the single
//!   source of constraint logic.
//! - **`REMOVE n.prop`** rejects when a `MinCount >= 1` rule covers
//!   the property — the removal would leave the node violating its
//!   declared cardinality.
//!
//! ## Deliberately deferred
//!
//! - `MaxCount` / cardinality > 1 → Cypher's inline map is always 0/1,
//!   so the max-count check is redundant at the AST layer (batch
//!   enforcement lives elsewhere).
//! - `Datatype` → the AST carries property values as raw text, so
//!   typing requires a full expression evaluator. Out of scope.
//! - Value references (`$param`, function calls, variables) → we only
//!   examine string literals because a parameterised value's
//!   compliance can't be settled without runtime substitution.
//! - `EdgeShape` / `CrossEntityShape` / `StateMachine` → each lands
//!   on its own phase; today's emitter surfaces them as `Info` so the
//!   rule author sees the rule is recognised but not yet enforced
//!   here.
//! - Whole-node `DELETE` / `DETACH DELETE` → cascade semantics belong
//!   to the storage engine, not the SHACL property surface; referential
//!   integrity for deletes lives with the relationship-layer
//!   enforcement.
//!
//! ## Why not fold into `OntologyValidator`?
//!
//! `OntologyValidator` checks *schema conformance* — does the label
//! exist, is the property declared on that type. SHACL rules encode
//! *domain semantics* — is this write allowed given policy. The two
//! run in the same phase (`PreRewriteOntology`) but are clearly
//! separable: a query can pass schema conformance and still violate a
//! SHACL rule, and the diagnostic validator names matter for UI
//! filtering and telemetry.

use std::collections::{HashMap, HashSet};
use std::fmt;

use ox_core::diagnostic::{diag, DiagnosticMessage};
use ox_core::graph_label::GraphLabel;
use ox_core::i18n::display_name_with_fallback;
use ox_core::types::PropertyType;
use ox_ontology::ir::{NodeTypeDef, OntologyIR, PropertyDef, PropertyId};
use ox_ontology::rule::{
    ConstraintTarget, EnforcementKind, RuleActivationKind, RuleDef, RuleKind, Severity,
    ShaclConstraint,
};
use ox_ontology::value_set::ValueSetId;

use crate::cypher::ast::{
    ClauseKind, CypherAst, CypherPatternElement, NodePattern, RemoveItem, SetItem,
};
use crate::cypher::validate::{
    CypherValidator, ValidateContext, ValidatePhase, ValidationIssue,
};

/// SHACL-rule enforcement against Cypher writes. See module docs for
/// the MVP constraint coverage.
///
/// The validator merges authored `OntologyIR.rules()` with rules
/// synthesised from `Required`-strength `PropertyBinding`s
/// (`OntologyIR::derive_implicit_rules`) so a binding's promise is
/// enforced at write time without the author copy-pasting the
/// constraint into a separate `RuleDef`.
///
/// **Dedup happens upstream**: `derive_implicit_rules` suppresses any
/// derivation whose `(node, property, signature)` is already covered
/// by an authored rule. The validator therefore iterates the merged
/// set without per-rule dedup logic — duplicate violations on the
/// LLM / admin surface would be a regression of that contract, not a
/// validator bug.
pub struct ShaclValidator {
    ontology: OntologyIR,
    /// Rules synthesised from `Required` bindings. Cached on
    /// construction so the per-validate hot path doesn't re-walk the
    /// IR's binding tree on every Cypher statement.
    derived_rules: Vec<RuleDef>,
    /// `ValueSetId` → expanded set of allowed code strings. Pre-built
    /// at construction so `InValueSet` enforcement is O(1) per write
    /// instead of re-running `expand_value_set` (which walks every
    /// composition rule + code-system code) on every Cypher statement.
    /// A query workload that touches `n` writes against a value set
    /// of `m` codes pays `O(n)` here vs `O(n·m)` without the cache.
    value_set_codes: HashMap<ValueSetId, HashSet<String>>,
}

impl ShaclValidator {
    pub fn new(ontology: OntologyIR) -> Self {
        let derived_rules = ontology.derive_implicit_rules();
        let value_set_codes = ontology
            .value_sets()
            .iter()
            .map(|vs| {
                let expansion = ox_ontology::value_set::expand_value_set(
                    vs,
                    ontology.code_systems(),
                );
                let codes = expansion
                    .codes
                    .iter()
                    .map(|cv| cv.code.clone())
                    .collect();
                (vs.id.clone(), codes)
            })
            .collect();
        Self {
            ontology,
            derived_rules,
            value_set_codes,
        }
    }
}

impl fmt::Debug for ShaclValidator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShaclValidator")
            .field("ontology_id", &self.ontology.id)
            .field("active_rules", &self.active_rules().count())
            .field("derived_rules", &self.derived_rules.len())
            .field("value_sets_cached", &self.value_set_codes.len())
            .finish()
    }
}

impl ShaclValidator {
    /// Rules live for the pre-execute write path: `Always` activation
    /// plus `Write` enforcement. `Read` rules fire in the result-shaping
    /// path (not implemented here); `Batch` rules fire in the
    /// data-quality scheduler.
    ///
    /// Authored rules and binding-derived rules are merged into the
    /// same iterator — the validator treats them identically. Derived
    /// rules are constructed with `Always`/`Write` so they always
    /// pass this filter; the explicit check stays for symmetry.
    fn active_rules(&self) -> impl Iterator<Item = &RuleDef> {
        self.ontology
            .rules()
            .iter()
            .chain(self.derived_rules.iter())
            .filter(|r| {
                matches!(r.enforcement, EnforcementKind::Write)
                    && matches!(r.activation, RuleActivationKind::Always)
            })
    }
}

impl CypherValidator for ShaclValidator {
    fn name(&self) -> &str {
        "shacl"
    }

    fn phase(&self) -> ValidatePhase {
        ValidatePhase::PreRewriteOntology
    }

    fn validate(&self, ast: &CypherAst, _ctx: &ValidateContext) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        let active: Vec<&RuleDef> = self.active_rules().collect();
        if active.is_empty() {
            return issues;
        }

        for statement in &ast.statements {
            // Statement-scoped variable → label resolution. Walk
            // every pattern-bearing clause (MATCH / OPTIONAL MATCH /
            // CREATE / MERGE) once and remember which labels each
            // variable carries so SET / REMOVE clauses later in the
            // same statement can resolve `n` back to its NodeType
            // without re-walking the AST per item.
            let variable_labels = statement.variable_labels();

            for clause in &statement.clauses {
                match clause.kind {
                    ClauseKind::Create | ClauseKind::Merge => {
                        for pattern in &clause.patterns {
                            for element in &pattern.elements {
                                if let CypherPatternElement::Node(node) = element {
                                    self.validate_node(&active, node, &mut issues);
                                }
                            }
                        }
                    }
                    ClauseKind::Set => {
                        for item in &clause.set_items {
                            self.validate_set_item(&active, &variable_labels, item, &mut issues);
                        }
                    }
                    ClauseKind::Remove => {
                        for item in &clause.remove_items {
                            self.validate_remove_item(
                                &active,
                                &variable_labels,
                                item,
                                &mut issues,
                            );
                        }
                    }
                    // MATCH / OPTIONAL MATCH / WHERE / RETURN /
                    // WITH / DELETE / DETACH DELETE / etc. — read-
                    // side or whole-node lifecycle, neither of which
                    // engages SHACL property-shape enforcement.
                    _ => {}
                }
            }
        }

        issues
    }
}

impl ShaclValidator {
    fn validate_node(
        &self,
        rules: &[&RuleDef],
        node: &NodePattern,
        issues: &mut Vec<ValidationIssue>,
    ) {
        for label_text in &node.labels {
            let Ok(label) = GraphLabel::new(label_text) else {
                continue;
            };
            let Some(node_type) = self.ontology.node_by_label(&label) else {
                // Schema conformance is `OntologyValidator`'s job — skip silently.
                continue;
            };
            for rule in rules {
                match &rule.kind {
                    RuleKind::NodeShape { target_node_type_id } => {
                        if *target_node_type_id == node_type.id {
                            for constraint in &rule.constraints {
                                self.check_node_level_constraint(
                                    rule, node_type, node, constraint, issues,
                                );
                            }
                        }
                    }
                    RuleKind::PropertyShape {
                        target_node_type_id,
                        target_property_id,
                    } => {
                        if *target_node_type_id != node_type.id {
                            continue;
                        }
                        let Some(prop) =
                            property_by_id(node_type, target_property_id)
                        else {
                            continue;
                        };
                        for constraint in &rule.constraints {
                            self.check_property_constraint(
                                rule, prop, node, constraint, issues,
                            );
                            // Node-level constraints declared inside a
                            // PropertyShape (e.g. UniqueKey targeting this
                            // node) still need to fire — route them
                            // through the node-level handler too.
                            self.check_node_level_constraint(
                                rule, node_type, node, constraint, issues,
                            );
                        }
                    }
                    // EdgeShape / CrossEntityShape / StateMachine —
                    // handled in their own phase; emit Info so rule authors
                    // see the rule is live without it silently doing nothing.
                    _ => {}
                }
            }
        }
    }

    fn check_property_constraint(
        &self,
        rule: &RuleDef,
        prop: &PropertyDef,
        node: &NodePattern,
        constraint: &ShaclConstraint,
        issues: &mut Vec<ValidationIssue>,
    ) {
        let key = prop.name.as_str();
        let value = lookup_property_value(node, key);
        match constraint {
            ShaclConstraint::MinCount { target, min } => {
                if *min == 0 {
                    return;
                }
                if !target_matches_property(target, prop) {
                    return;
                }
                if value.is_none() {
                    let rn = rule_label(rule);
                    issues.push(build_issue(
                        rule,
                        diag("runtime.cypher.shacl.min_count_missing")
                            .with("property", key)
                            .with("rule_id", rule.id.as_str())
                            .with("rule_name", &rule.name)
                            .with("min", *min)
                            .message(format!(
                                "property `{key}` is required by rule `{rn}` \
                                 (minCount={min}) but missing from the write"
                            )),
                        Some(node.span),
                    ));
                }
            }
            ShaclConstraint::InValueSet {
                target,
                value_set_id,
            } => {
                if !target_matches_property(target, prop) {
                    return;
                }
                let Some(raw) = value else {
                    return;
                };
                // Three observed-literal shapes the value set comparison
                // is meaningful for:
                //
                // 1. string literal `'foo'` → strip quotes, compare to
                //    the `codes` set.
                // 2. numeric / bool literal `42` / `true` → raw text
                //    matches a code verbatim. Value sets carry their
                //    `codes` as strings on the wire (see
                //    `CodeSystemDef.codes[].code: String`), so a rule
                //    author writing `"42"` / `"true"` matches against
                //    the literal `42` / `true` written in the query —
                //    same wire shape as `HasValue`.
                // 3. param / function call / identifier / null →
                //    opaque, silent skip; driver runtime takes the
                //    call.
                let literal: String = if let Some(s) = parse_string_literal(raw) {
                    s
                } else if classify_literal_type(raw).is_some_and(|t| {
                    matches!(
                        t,
                        PropertyType::Int | PropertyType::Float | PropertyType::Bool
                    )
                }) {
                    raw.trim().to_string()
                } else {
                    return;
                };
                // Hit the pre-built cache (O(1)) instead of re-running
                // `expand_value_set` for every literal. A dangling id
                // is a rule-authoring error; surface as Info and skip
                // enforcement rather than blocking every caller.
                let Some(allowed) = self.value_set_codes.get(value_set_id) else {
                    let rn = rule_label(rule);
                    issues.push(ValidationIssue::info(
                        "shacl",
                        diag("runtime.cypher.shacl.unknown_value_set_skipped")
                            .with("rule_id", rule.id.as_str())
                            .with("rule_name", &rule.name)
                            .with("value_set_id", value_set_id.as_str())
                            .with("property", key)
                            .message(format!(
                                "rule `{rn}` references unknown value set `{value_set_id}` \
                                 and was skipped for property `{key}`"
                            )),
                    ));
                    return;
                };
                if !allowed.contains(&literal) {
                    let vs = self.ontology.value_set_by_id(value_set_id);
                    let (vs_name_param, vs_name_en) = match vs {
                        Some(v) => display_name_with_fallback(&v.display_name, v.name.as_str()),
                        None => (
                            serde_json::Value::String(value_set_id.as_str().to_string()),
                            value_set_id.as_str(),
                        ),
                    };
                    let mut sample: Vec<&str> =
                        allowed.iter().take(8).map(String::as_str).collect();
                    sample.sort_unstable();
                    let ellipsis = if allowed.len() > sample.len() { ", …" } else { "" };
                    let rn = rule_label(rule);
                    issues.push(build_issue(
                        rule,
                        diag("runtime.cypher.shacl.value_not_in_set")
                            .with("property", key)
                            .with("value", raw)
                            .with("rule_id", rule.id.as_str())
                            .with("rule_name", &rule.name)
                            .with("value_set_id", value_set_id.as_str())
                            .with("value_set_name", vs_name_param)
                            .with("sample", serde_json::Value::from(sample.clone()))
                            .message(format!(
                                "property `{key}` = {raw:?} violates rule `{rn}` \
                                 (value set `{vs_name_en}` allows: {sample:?}{ellipsis})"
                            )),
                        Some(node.span),
                    ));
                }
            }
            ShaclConstraint::MatchesPattern {
                target,
                notation_pattern_id,
            } => {
                if !target_matches_property(target, prop) {
                    return;
                }
                let Some(raw) = value else {
                    return;
                };
                let Some(literal) = parse_string_literal(raw) else {
                    return;
                };
                let Some(np) = self.ontology.notation_pattern_by_id(notation_pattern_id) else {
                    let rn = rule_label(rule);
                    issues.push(ValidationIssue::info(
                        "shacl",
                        diag("runtime.cypher.shacl.unknown_notation_pattern_skipped")
                            .with("rule_id", rule.id.as_str())
                            .with("rule_name", &rule.name)
                            .with("notation_pattern_id", notation_pattern_id.as_str())
                            .with("property", key)
                            .message(format!(
                                "rule `{rn}` references unknown notation pattern \
                                 `{notation_pattern_id}` and was skipped for property `{key}`"
                            )),
                    ));
                    return;
                };
                // Resolve `CodeFromSet` components against the active
                // ontology's value sets; other component kinds ignore
                // the resolver.
                let ontology_ref = &self.ontology;
                let code_resolver = |vs_id: &ox_ontology::value_set::ValueSetId, code: &str| {
                    let Some(vs) = ontology_ref.value_set_by_id(vs_id) else {
                        return false;
                    };
                    let expansion = ox_ontology::value_set::expand_value_set(
                        vs,
                        ontology_ref.code_systems(),
                    );
                    expansion.codes.iter().any(|cv| cv.code == code)
                };
                if let Err(err) = np.validate(&literal, code_resolver) {
                    let rn = rule_label(rule);
                    let (np_name_param, np_name_en) =
                        display_name_with_fallback(&np.display_name, np.name.as_str());
                    issues.push(build_issue(
                        rule,
                        diag("runtime.cypher.shacl.notation_pattern_mismatch")
                            .with("property", key)
                            .with("value", raw)
                            .with("rule_id", rule.id.as_str())
                            .with("rule_name", &rule.name)
                            .with("notation_pattern_id", notation_pattern_id.as_str())
                            .with("notation_pattern_name", np_name_param)
                            .with("error", err.to_string())
                            .message(format!(
                                "property `{key}` = {raw:?} does not match notation \
                                 pattern `{np_name_en}` required by rule `{rn}` ({err})"
                            )),
                        Some(node.span),
                    ));
                }
            }
            ShaclConstraint::LessThan { target, other_property } => {
                check_property_pair_predicate(
                    rule,
                    prop,
                    node,
                    target,
                    other_property,
                    PropertyPairOp::LessThan,
                    &self.ontology,
                    issues,
                );
            }
            ShaclConstraint::Equals { target, other_property } => {
                check_property_pair_predicate(
                    rule,
                    prop,
                    node,
                    target,
                    other_property,
                    PropertyPairOp::Equals,
                    &self.ontology,
                    issues,
                );
            }
            // Constraint kinds handled at the node-shape level (see
            // check_node_level_constraint) or at a later phase.
            ShaclConstraint::Closed { .. }
            | ShaclConstraint::Disjoint { .. }
            | ShaclConstraint::UniqueKey { .. }
            | ShaclConstraint::UniqueLang { .. } => {}
            ShaclConstraint::Datatype { target, expected } => {
                if !target_matches_property(target, prop) {
                    return;
                }
                let Some(raw) = value else {
                    return;
                };
                // List-typed expectation walks each element against
                // the inner type. The classifier rejects bracket-
                // led tokens for the scalar path, so without this
                // arm a `SET tags = ['a', 1]` slipped through a
                // `List { element: String }` declaration silently.
                if let PropertyType::List { element } = expected {
                    let Some(items) = parse_list_literal(raw) else {
                        return;
                    };
                    for (idx, item) in items.iter().enumerate() {
                        let Some(item_type) = classify_literal_type(item) else {
                            // Param / function / ident inside the
                            // list — driver takes it.
                            continue;
                        };
                        if type_assignable(&item_type, element) {
                            continue;
                        }
                        let rn = rule_label(rule);
                        let expected_label = property_type_label(element);
                        let observed_label = property_type_label(&item_type);
                        issues.push(build_issue(
                            rule,
                            diag("runtime.cypher.shacl.list_element_datatype_mismatch")
                                .with("property", key)
                                .with("rule_id", rule.id.as_str())
                                .with("rule_name", &rule.name)
                                .with("expected_type", expected_label)
                                .with("observed_type", observed_label)
                                .with("element_value", item.clone())
                                .with("element_index", idx as u64)
                                .message(format!(
                                    "list element [{idx}] of property `{key}` = {item} violates rule `{rn}` — expected {expected_label}, got {observed_label}"
                                )),
                            Some(node.span),
                        ));
                    }
                    return;
                }
                let Some(literal_type) = classify_literal_type(raw) else {
                    // Parameter, function call, identifier reference,
                    // null, opaque list/map — cannot decide at AST
                    // time. Driver runtime takes the call.
                    return;
                };
                if type_assignable(&literal_type, expected) {
                    return;
                }
                let rn = rule_label(rule);
                let expected_label = property_type_label(expected);
                let observed_label = property_type_label(&literal_type);
                issues.push(build_issue(
                    rule,
                    diag("runtime.cypher.shacl.datatype_mismatch")
                        .with("property", key)
                        .with("rule_id", rule.id.as_str())
                        .with("rule_name", &rule.name)
                        .with("expected_type", expected_label)
                        .with("observed_type", observed_label)
                        .with("value", raw)
                        .message(format!(
                            "property `{key}` = {raw} violates rule `{rn}` — expected {expected_label}, got {observed_label}"
                        )),
                    Some(node.span),
                ));
            }
            ShaclConstraint::MinInclusive { target, min } => {
                if !target_matches_property(target, prop) {
                    return;
                }
                let Some(raw) = value else {
                    return;
                };
                let Some(observed) = parse_numeric_literal(raw) else {
                    // String / param / function call — cannot decide
                    // numerically at AST time. Driver runtime takes
                    // it (and a Datatype rule on the same property
                    // would catch the type mismatch separately).
                    return;
                };
                if observed >= *min {
                    return;
                }
                let rn = rule_label(rule);
                issues.push(build_issue(
                    rule,
                    diag("runtime.cypher.shacl.min_inclusive_violation")
                        .with("property", key)
                        .with("rule_id", rule.id.as_str())
                        .with("rule_name", &rule.name)
                        .with("min", *min)
                        .with("value", raw)
                        .message(format!(
                            "property `{key}` = {raw} violates rule `{rn}` — value must be ≥ {min}"
                        )),
                    Some(node.span),
                ));
            }
            ShaclConstraint::MaxInclusive { target, max } => {
                if !target_matches_property(target, prop) {
                    return;
                }
                let Some(raw) = value else {
                    return;
                };
                let Some(observed) = parse_numeric_literal(raw) else {
                    return;
                };
                if observed <= *max {
                    return;
                }
                let rn = rule_label(rule);
                issues.push(build_issue(
                    rule,
                    diag("runtime.cypher.shacl.max_inclusive_violation")
                        .with("property", key)
                        .with("rule_id", rule.id.as_str())
                        .with("rule_name", &rule.name)
                        .with("max", *max)
                        .with("value", raw)
                        .message(format!(
                            "property `{key}` = {raw} violates rule `{rn}` — value must be ≤ {max}"
                        )),
                    Some(node.span),
                ));
            }
            ShaclConstraint::MinLength { target, min } => {
                if !target_matches_property(target, prop) {
                    return;
                }
                let Some(raw) = value else {
                    return;
                };
                let Some(literal) = parse_string_literal(raw) else {
                    return;
                };
                let observed = literal.chars().count();
                if observed >= *min as usize {
                    return;
                }
                let rn = rule_label(rule);
                issues.push(build_issue(
                    rule,
                    diag("runtime.cypher.shacl.min_length_violation")
                        .with("property", key)
                        .with("rule_id", rule.id.as_str())
                        .with("rule_name", &rule.name)
                        .with("min", *min)
                        .with("observed", observed as u64)
                        .with("value", raw)
                        .message(format!(
                            "property `{key}` = {raw} violates rule `{rn}` — string length must be ≥ {min} (got {observed})"
                        )),
                    Some(node.span),
                ));
            }
            ShaclConstraint::MaxLength { target, max } => {
                if !target_matches_property(target, prop) {
                    return;
                }
                let Some(raw) = value else {
                    return;
                };
                let Some(literal) = parse_string_literal(raw) else {
                    return;
                };
                let observed = literal.chars().count();
                if observed <= *max as usize {
                    return;
                }
                let rn = rule_label(rule);
                issues.push(build_issue(
                    rule,
                    diag("runtime.cypher.shacl.max_length_violation")
                        .with("property", key)
                        .with("rule_id", rule.id.as_str())
                        .with("rule_name", &rule.name)
                        .with("max", *max)
                        .with("observed", observed as u64)
                        .with("value", raw)
                        .message(format!(
                            "property `{key}` = {raw} violates rule `{rn}` — string length must be ≤ {max} (got {observed})"
                        )),
                    Some(node.span),
                ));
            }
            ShaclConstraint::HasValue { target, value: expected } => {
                if !target_matches_property(target, prop) {
                    return;
                }
                let Some(raw) = value else {
                    return;
                };
                // Three comparison shapes:
                // 1. string literal `'foo'` → unquote, compare to rule's
                //    `expected` text directly (rule authors don't carry
                //    literal quotes through the IR).
                // 2. numeric / bool literal `42` / `true` → raw text
                //    matches `expected` verbatim (rule author is
                //    expected to write `"42"` / `"true"` in the rule
                //    body — same wire shape as `InValueSet.codes`).
                // 3. param / function / identifier — opaque, silent
                //    skip; driver runtime takes the call.
                let observed: Option<String> = if let Some(literal) =
                    parse_string_literal(raw)
                {
                    Some(literal)
                } else if classify_literal_type(raw)
                    .is_some_and(|t| matches!(t, PropertyType::Int | PropertyType::Float | PropertyType::Bool))
                {
                    Some(raw.to_string())
                } else {
                    None
                };
                let Some(observed) = observed else {
                    return;
                };
                if observed == *expected {
                    return;
                }
                let rn = rule_label(rule);
                issues.push(build_issue(
                    rule,
                    diag("runtime.cypher.shacl.has_value_violation")
                        .with("property", key)
                        .with("rule_id", rule.id.as_str())
                        .with("rule_name", &rule.name)
                        .with("expected", expected.clone())
                        .with("value", raw)
                        .message(format!(
                            "property `{key}` = {raw} violates rule `{rn}` — must be `{expected}`"
                        )),
                    Some(node.span),
                ));
            }
            ShaclConstraint::MaxCount { target, max } => {
                // The AST-decidable axis: when the SET / inline
                // value is a list literal, we can count its
                // elements directly. The single-value case
                // (`SET p.tags = 'one'`, `SET p.score = 42`)
                // implies count=1 and only fails MaxCount=0,
                // which is itself an authoring error — leaving
                // it to the runtime is fine. Cross-write
                // multiplicity (`MATCH (p) WITH p UNION
                // MATCH (q) WITH q SET p.tag = 'a'…`) cannot be
                // decided here either; the dispatcher's final
                // arm documents the deliberate gap, and a
                // future runtime-side companion check belongs
                // in the bolt driver.
                if !target_matches_property(target, prop) {
                    return;
                }
                let Some(raw) = value else {
                    return;
                };
                let Some(items) = parse_list_literal(raw) else {
                    return;
                };
                if items.len() <= *max as usize {
                    return;
                }
                let rn = rule_label(rule);
                issues.push(build_issue(
                    rule,
                    diag("runtime.cypher.shacl.max_count_violation")
                        .with("property", key)
                        .with("rule_id", rule.id.as_str())
                        .with("rule_name", &rule.name)
                        .with("max", *max)
                        .with("observed", items.len() as u64)
                        .with("value", raw)
                        .message(format!(
                            "property `{key}` = {raw} violates rule `{rn}` — list length must be ≤ {max} (got {})",
                            items.len()
                        )),
                    Some(node.span),
                ));
            }
            ShaclConstraint::Or { branches } => {
                // The value satisfies the constraint when at least
                // one branch accepts. Recurse into each branch with
                // a private issue buffer; if any branch produces no
                // issues against this property write, the OR
                // succeeds and the recorded issues are dropped.
                // Empty `branches` is a vacuously failing
                // constraint per the rule's documentation — surface
                // a typed diagnostic so the operator notices the
                // empty authoring rather than silently accepting
                // every value.
                if branches.is_empty() {
                    let rn = rule_label(rule);
                    issues.push(build_issue(
                        rule,
                        diag("runtime.cypher.shacl.or_empty_branches")
                            .with("property", key)
                            .with("rule_id", rule.id.as_str())
                            .with("rule_name", &rule.name)
                            .message(format!(
                                "property `{key}` write rejected by rule `{rn}` — Or constraint has no branches"
                            )),
                        Some(node.span),
                    ));
                    return;
                }
                let mut branch_issues: Vec<Vec<ValidationIssue>> =
                    Vec::with_capacity(branches.len());
                for branch in branches {
                    let mut buf = Vec::new();
                    self.check_property_constraint(rule, prop, node, branch, &mut buf);
                    if buf.is_empty() {
                        return;
                    }
                    branch_issues.push(buf);
                }
                // No branch satisfied — surface a single OR-level
                // diagnostic instead of the unhelpful per-branch
                // pile so the FE renders one message naming the
                // outer rule. Detail buffers carry per-branch
                // diagnostics for operators who want to drill in.
                let rn = rule_label(rule);
                let branch_count = branches.len();
                issues.push(build_issue(
                    rule,
                    diag("runtime.cypher.shacl.or_no_branch_satisfied")
                        .with("property", key)
                        .with("rule_id", rule.id.as_str())
                        .with("rule_name", &rule.name)
                        .with("branch_count", branch_count as u64)
                        .with("value", value.unwrap_or("<missing>"))
                        .message(format!(
                            "property `{key}` write rejected by rule `{rn}` — \
                             no Or branch ({branch_count}) accepted the value"
                        )),
                    Some(node.span),
                ));
            }
            ShaclConstraint::And { branches } => {
                // Empty `branches` is vacuously *true* per SHACL
                // spec § 4.6.1 — silently accept and return. A
                // non-empty `And` recurses into every branch and
                // every branch's diagnostics propagate up; the
                // shape fails iff any one branch fails.
                if branches.is_empty() {
                    return;
                }
                for branch in branches {
                    self.check_property_constraint(rule, prop, node, branch, issues);
                }
            }
            ShaclConstraint::Not { inner } => {
                // The inner constraint must NOT hold for the value.
                // Recurse with a private buffer; if the inner check
                // produces issues, the value violates `inner` so
                // the outer `Not` succeeds. If the inner check is
                // clean, `Not` fails and surfaces a single
                // diagnostic (per-branch issues are dropped — the
                // operator authored "must not", so the inner
                // detail isn't actionable).
                let mut inner_issues = Vec::new();
                self.check_property_constraint(rule, prop, node, inner, &mut inner_issues);
                if inner_issues.is_empty() {
                    let rn = rule_label(rule);
                    issues.push(build_issue(
                        rule,
                        diag("runtime.cypher.shacl.not_violation")
                            .with("property", key)
                            .with("rule_id", rule.id.as_str())
                            .with("rule_name", &rule.name)
                            .with("value", value.unwrap_or("<missing>"))
                            .message(format!(
                                "property `{key}` write rejected by rule `{rn}` — \
                                 value satisfies a forbidden constraint"
                            )),
                        Some(node.span),
                    ));
                }
            }
            ShaclConstraint::Xone { branches } => {
                // Exactly one branch must accept. Empty branches
                // is vacuously failing (mirrors `Or` empty-list
                // semantics — there is no satisfying branch).
                if branches.is_empty() {
                    let rn = rule_label(rule);
                    issues.push(build_issue(
                        rule,
                        diag("runtime.cypher.shacl.xone_empty_branches")
                            .with("property", key)
                            .with("rule_id", rule.id.as_str())
                            .with("rule_name", &rule.name)
                            .message(format!(
                                "property `{key}` write rejected by rule `{rn}` — Xone constraint has no branches"
                            )),
                        Some(node.span),
                    ));
                    return;
                }
                let mut accepted = 0u32;
                for branch in branches {
                    let mut buf = Vec::new();
                    self.check_property_constraint(rule, prop, node, branch, &mut buf);
                    if buf.is_empty() {
                        accepted += 1;
                    }
                }
                if accepted != 1 {
                    let rn = rule_label(rule);
                    let branch_count = branches.len();
                    issues.push(build_issue(
                        rule,
                        diag("runtime.cypher.shacl.xone_violation")
                            .with("property", key)
                            .with("rule_id", rule.id.as_str())
                            .with("rule_name", &rule.name)
                            .with("branch_count", branch_count as u64)
                            .with("matched_count", accepted as u64)
                            .with("value", value.unwrap_or("<missing>"))
                            .message(format!(
                                "property `{key}` write rejected by rule `{rn}` — \
                                 expected exactly 1 of {branch_count} Xone branches to accept, \
                                 got {accepted}"
                            )),
                        Some(node.span),
                    ));
                }
            }
            ShaclConstraint::QualifiedValueShape { .. } => {
                // `sh:qualifiedValueShape` constrains the *count*
                // of values on the property satisfying an inner
                // shape — not whether one observed write satisfies
                // it. The AST validator only sees one write at a
                // time, so the cardinality cannot be decided
                // pre-execute. Same rationale as `MaxCount`
                // (cypher/CLAUDE.md "Deliberately not enforced at
                // AST layer"): a future runtime-side companion
                // check belongs in the bolt driver. Silent skip
                // here keeps the AST validator from emitting false
                // positives on every write to a qualified property.
            }
        }
    }

    /// Resolve `SET n.prop = value` against every label `n` carries
    /// and run the property-level constraints (InValueSet,
    /// MatchesPattern, LessThan, Equals) against the new value. The
    /// raw `value_text` slips into the same `lookup` shape the
    /// CREATE / MERGE path uses — a synthetic single-property node
    /// pattern keeps the existing `check_property_constraint`
    /// machinery as the single source of enforcement logic.
    fn validate_set_item(
        &self,
        rules: &[&RuleDef],
        variable_labels: &HashMap<String, Vec<String>>,
        item: &SetItem,
        issues: &mut Vec<ValidationIssue>,
    ) {
        let Some(labels) = variable_labels.get(&item.variable) else {
            // Variable not bound to a labelled node in this statement
            // (e.g. an alias from WITH, an aggregate result). Schema
            // conformance is `OntologyValidator`'s job — skip silently.
            return;
        };
        let synth = NodePattern {
            variable: Some(item.variable.clone()),
            labels: labels.clone(),
            properties: vec![(item.property.clone(), item.value_text.clone())],
            span: item.span,
        };
        for label_text in labels {
            let Ok(label) = GraphLabel::new(label_text) else {
                continue;
            };
            let Some(node_type) = self.ontology.node_by_label(&label) else {
                continue;
            };
            let Some(prop) = node_type
                .properties
                .iter()
                .find(|p| p.name.as_str() == item.property)
            else {
                continue;
            };
            for rule in rules {
                let RuleKind::PropertyShape {
                    target_node_type_id,
                    target_property_id,
                } = &rule.kind
                else {
                    continue;
                };
                if *target_node_type_id != node_type.id || *target_property_id != prop.id {
                    continue;
                }
                for constraint in &rule.constraints {
                    self.check_property_constraint(rule, prop, &synth, constraint, issues);
                }
            }
        }
    }

    /// Resolve `REMOVE n.prop` against every label `n` carries and
    /// reject when a `MinCount >= 1` PropertyShape covers the
    /// property — the removal would leave the node violating the
    /// declared cardinality. Other constraint kinds (InValueSet,
    /// MatchesPattern, …) don't engage on removals because there is
    /// no value left to inspect.
    fn validate_remove_item(
        &self,
        rules: &[&RuleDef],
        variable_labels: &HashMap<String, Vec<String>>,
        item: &RemoveItem,
        issues: &mut Vec<ValidationIssue>,
    ) {
        let Some(labels) = variable_labels.get(&item.variable) else {
            return;
        };
        for label_text in labels {
            let Ok(label) = GraphLabel::new(label_text) else {
                continue;
            };
            let Some(node_type) = self.ontology.node_by_label(&label) else {
                continue;
            };
            let Some(prop) = node_type
                .properties
                .iter()
                .find(|p| p.name.as_str() == item.property)
            else {
                continue;
            };
            for rule in rules {
                let RuleKind::PropertyShape {
                    target_node_type_id,
                    target_property_id,
                } = &rule.kind
                else {
                    continue;
                };
                if *target_node_type_id != node_type.id || *target_property_id != prop.id {
                    continue;
                }
                for constraint in &rule.constraints {
                    let ShaclConstraint::MinCount { target, min } = constraint else {
                        continue;
                    };
                    if *min == 0 {
                        continue;
                    }
                    if !target_matches_property(target, prop) {
                        continue;
                    }
                    let rn = rule_label(rule);
                    let key = prop.name.as_str();
                    issues.push(build_issue(
                        rule,
                        diag("runtime.cypher.shacl.min_count_removed")
                            .with("property", key)
                            .with("rule_id", rule.id.as_str())
                            .with("rule_name", &rule.name)
                            .with("min", *min)
                            .message(format!(
                                "REMOVE on `{}.{key}` violates rule `{rn}` (minCount={min}) — \
                                 the property is required but the write would clear it",
                                label.as_str()
                            )),
                        Some(item.span),
                    ));
                }
            }
        }
    }

    /// Node-level constraints — `sh:closed`, `sh:disjoint`,
    /// `sh:uniqueKey`, `sh:uniqueLang`. These don't belong to a
    /// single `PropertyDef`; they apply to the whole write pattern.
    fn check_node_level_constraint(
        &self,
        rule: &RuleDef,
        node_type: &NodeTypeDef,
        node: &NodePattern,
        constraint: &ShaclConstraint,
        issues: &mut Vec<ValidationIssue>,
    ) {
        match constraint {
            ShaclConstraint::Closed {
                target: _,
                allowed_properties,
            } => {
                // The "closed set" is the union of the rule's explicit
                // `allowed_properties` and every PropertyDef on the
                // node type itself — the SHACL spec says a closed
                // shape still permits its declared properties.
                let mut allowed: std::collections::HashSet<&str> =
                    allowed_properties.iter().map(|k| k.as_str()).collect();
                for p in &node_type.properties {
                    allowed.insert(p.name.as_str());
                }
                for (key, _value) in &node.properties {
                    if !allowed.contains(key.as_str()) {
                        let rn = rule_label(rule);
                        let label = node_type.label.as_str();
                        issues.push(build_issue(
                            rule,
                            diag("runtime.cypher.shacl.closed_shape_extra_property")
                                .with("property", key.as_str())
                                .with("label", label)
                                .with("rule_id", rule.id.as_str())
                                .with("rule_name", &rule.name)
                                .message(format!(
                                    "property `{key}` is not permitted on `{label}` \
                                     (closed shape rule `{rn}`)"
                                )),
                            Some(node.span),
                        ));
                    }
                }
            }
            ShaclConstraint::Disjoint { a, b } => {
                // AST-level enforcement: when the write pattern assigns
                // **both** targets as literal values on the same node,
                // they must differ. Cross-instance disjointness needs a
                // runtime scan and belongs to the DQ scheduler.
                let Some(a_prop_id) = disjoint_target_property(a) else {
                    return;
                };
                let Some(b_prop_id) = disjoint_target_property(b) else {
                    return;
                };
                let Some(a_prop) = property_by_id(node_type, a_prop_id) else {
                    return;
                };
                let Some(b_prop) = property_by_id(node_type, b_prop_id) else {
                    return;
                };
                let (Some(a_raw), Some(b_raw)) = (
                    lookup_property_value(node, a_prop.name.as_str()),
                    lookup_property_value(node, b_prop.name.as_str()),
                ) else {
                    return;
                };
                let (Some(a_literal), Some(b_literal)) =
                    (parse_string_literal(a_raw), parse_string_literal(b_raw))
                else {
                    return;
                };
                if a_literal == b_literal {
                    let ak = a_prop.name.as_str();
                    let bk = b_prop.name.as_str();
                    let rn = rule_label(rule);
                    issues.push(build_issue(
                        rule,
                        diag("runtime.cypher.shacl.disjoint_pair_present")
                            .with("property_a", ak)
                            .with("property_b", bk)
                            .with("value", a_raw)
                            .with("rule_id", rule.id.as_str())
                            .with("rule_name", &rule.name)
                            .message(format!(
                                "properties `{ak}` and `{bk}` carry the same value \
                                 {a_raw:?} but rule `{rn}` declares them disjoint"
                            )),
                        Some(node.span),
                    ));
                }
            }
            ShaclConstraint::UniqueKey {
                target_node_type_id,
                property_keys,
            } => {
                // AST-level enforcement checks that every referenced
                // property key resolves to a PropertyDef on the target
                // node type. Cross-row uniqueness is enforced by the
                // DB index declared from this rule.
                if *target_node_type_id != node_type.id {
                    return;
                }
                for key in property_keys {
                    let exists = node_type
                        .properties
                        .iter()
                        .any(|p| p.name.as_str() == key.as_str());
                    if !exists {
                        let rn = rule_label(rule);
                        let label = node_type.label.as_str();
                        let key = key.as_str();
                        issues.push(build_issue(
                            rule,
                            diag("runtime.cypher.shacl.unique_key_unknown_property")
                                .with("property", key)
                                .with("label", label)
                                .with("rule_id", rule.id.as_str())
                                .with("rule_name", &rule.name)
                                .message(format!(
                                    "unique-key rule `{rn}` references unknown property `{key}` \
                                     on `{label}`"
                                )),
                            Some(node.span),
                        ));
                    }
                }
            }
            ShaclConstraint::UniqueLang { target } => {
                // Localised properties store a {locale: value} map.
                // Cypher literal map keys must already be unique
                // (the parser rejects duplicates), so AST-level
                // enforcement is a no-op: the constraint is meaningful
                // at the serialisation / DQ boundary, not the write
                // AST. Flag only if the targeted property is not
                // declared `is_localized` — that's a rule-authoring
                // smell worth surfacing as Info.
                let Some(prop_id) = disjoint_target_property(target) else {
                    return;
                };
                let Some(prop) = property_by_id(node_type, prop_id) else {
                    return;
                };
                if !prop.is_localized {
                    let rn = rule_label(rule);
                    let key = prop.name.as_str();
                    issues.push(ValidationIssue::info(
                        "shacl",
                        diag("runtime.cypher.shacl.unique_lang_on_non_localized")
                            .with("property", key)
                            .with("rule_id", rule.id.as_str())
                            .with("rule_name", &rule.name)
                            .message(format!(
                                "rule `{rn}` enforces uniqueLang on `{key}`, but the \
                                 property is not declared `is_localized`"
                            )),
                    ));
                }
            }
            // Property-level constraints live in
            // check_property_constraint — the node-level pass ignores
            // them.
            _ => {}
        }
    }
}

/// Pull the `property_id` from a `ConstraintTarget`; `Inherit` /
/// `NodeType` / `EdgeLabel` targets don't resolve to a property.
fn disjoint_target_property(target: &ConstraintTarget) -> Option<&PropertyId> {
    match target {
        ConstraintTarget::Property { property_id, .. } => Some(property_id),
        ConstraintTarget::Inherit
        | ConstraintTarget::NodeType { .. }
        | ConstraintTarget::EdgeLabel { .. } => None,
    }
}

/// `Inherit` target = the property the enclosing `PropertyShape` names.
/// Explicit `Property { ... }` target = an out-of-shape reference; we
/// match by id to keep cross-shape constraints functioning.
fn target_matches_property(target: &ConstraintTarget, prop: &PropertyDef) -> bool {
    match target {
        ConstraintTarget::Inherit => true,
        ConstraintTarget::Property { property_id, .. } => *property_id == prop.id,
        ConstraintTarget::NodeType { .. } | ConstraintTarget::EdgeLabel { .. } => false,
    }
}

fn lookup_property_value<'a>(node: &'a NodePattern, key: &str) -> Option<&'a str> {
    node.properties
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// Classify a SET / inline-pattern `value_text` slice into its
/// concrete literal type. Returns `None` when the value is anything
/// the validator cannot decide at AST time (parameter `$x`,
/// function call `coalesce(...)`, property reference `n.other`,
/// `null`, identifier alias). The Datatype enforcement walks this
/// classifier and skips the unknown cases — false-positive avoidance
/// over false-negative coverage, mirroring the rest of the validator.
///
/// The classifier is conservative on widening: an integer literal
/// could lawfully assign to a `Float` slot, but that's the SHACL
/// compatibility test's call (see [`type_assignable`]). Here we
/// emit only the narrowest match — `42` → `Int`, never `Float`.
pub(crate) fn classify_literal_type(raw: &str) -> Option<PropertyType> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    // String literal: 'foo' / "foo" — quotes already stripped by
    // parse_string_literal, but here we recognise the wrapper
    // straight from the raw token so we don't double-allocate just
    // to classify.
    let first = t.chars().next()?;
    let last = t.chars().next_back()?;
    if (first, last) == ('\'', '\'') || (first, last) == ('"', '"') {
        return Some(PropertyType::String);
    }
    // Boolean keywords (case-insensitive — the lexer keeps original
    // casing in the token text).
    if t.eq_ignore_ascii_case("true") || t.eq_ignore_ascii_case("false") {
        return Some(PropertyType::Bool);
    }
    // `null` is "absence of value", not a typed value — every type
    // accepts it (subject to MinCount / nullable rules elsewhere).
    if t.eq_ignore_ascii_case("null") {
        return None;
    }
    // Cypher list literal `[...]` is opaque at this layer — element
    // typing would need a recursive parse that the validator
    // intentionally avoids. Skip rather than over-reject.
    if first == '[' || first == '{' {
        return None;
    }
    // Parameter / function-call — opaque references the validator
    // cannot decide at AST time. `(` indicates a call (`coalesce(...)`),
    // `$` introduces a parameter.
    if first == '$' || t.contains('(') {
        return None;
    }
    // Numeric — try Int first, then Float. The classifier emits the
    // narrowest match so the compatibility test can apply widening
    // explicitly (Int → Float OK, Float → Int reject). Run parse
    // BEFORE the property-access guard below — `3.14` is a Float
    // literal even though it carries a `.`, while `n.other` is an
    // identifier reference that won't parse as a number.
    if t.parse::<i64>().is_ok() {
        return Some(PropertyType::Int);
    }
    if t.parse::<f64>().is_ok() {
        return Some(PropertyType::Float);
    }
    // Property access `n.other` or qualified identifier — opaque
    // reference, skip. Anything that survived the numeric parse but
    // still carries a `.` is a non-literal access path.
    if t.contains('.') {
        return None;
    }
    None
}

/// Stable wire-string label for a `PropertyType` used in
/// diagnostic params. The locale-aware rendering happens in the
/// FE i18n catalogue; here we emit the snake_case identifier
/// (`"int"`, `"date_time"`, `"list"`) so the ICU select arm has a
/// fixed key to switch on. List elements are not unfolded — the
/// outer kind is the actionable bit for an operator chasing a
/// SET-clause typo.
pub(crate) fn property_type_label(ty: &PropertyType) -> &'static str {
    match ty {
        PropertyType::Bool => "bool",
        PropertyType::Int => "int",
        PropertyType::Float => "float",
        PropertyType::String => "string",
        PropertyType::Date => "date",
        PropertyType::DateTime => "date_time",
        PropertyType::Duration => "duration",
        PropertyType::Bytes => "bytes",
        PropertyType::List { .. } => "list",
        PropertyType::Map => "map",
    }
}

/// Parse a SET / inline literal as a Cypher list literal — `[]`,
/// `[1, 2, 3]`, `['a, b', 'c']`, `[[1, 2], [3, 4]]` — and return
/// each top-level element's raw text. Returns `None` for anything
/// that isn't a bracketed list (single value, parameter, function
/// call, etc.).
///
/// The split is brackets-aware: commas inside a quoted string,
/// nested list, map literal, or function call do not separate
/// elements. Escape sequences inside string literals (`\'`, `\\`)
/// are preserved verbatim — this helper is for *splitting*, not
/// *unescaping*; downstream callers feed each element back through
/// the existing `parse_string_literal` / `classify_literal_type`
/// helpers when they need the unwrapped value.
///
/// Iteration walks byte-by-byte, but every character that affects
/// nesting (`[`, `]`, `(`, `)`, `{`, `}`, `'`, `"`, `,`, `\`) is
/// ASCII, so the byte/char boundary alignment is safe even when
/// the string contains multi-byte UTF-8 (e.g. `['한글']`).
pub(crate) fn parse_list_literal(raw: &str) -> Option<Vec<String>> {
    let t = raw.trim();
    if !t.starts_with('[') || !t.ends_with(']') {
        return None;
    }
    let inner = &t[1..t.len() - 1];
    if inner.trim().is_empty() {
        return Some(Vec::new());
    }
    let mut elements = Vec::new();
    let bytes = inner.as_bytes();
    let mut depth_paren: i32 = 0;
    let mut depth_bracket: i32 = 0;
    let mut depth_brace: i32 = 0;
    let mut in_squote = false;
    let mut in_dquote = false;
    let mut start = 0usize;
    let mut prev: u8 = 0;
    for (i, &c) in bytes.iter().enumerate() {
        let escaped = prev == b'\\';
        match c {
            b'\'' if !in_dquote && !escaped => in_squote = !in_squote,
            b'"' if !in_squote && !escaped => in_dquote = !in_dquote,
            b'(' if !in_squote && !in_dquote => depth_paren += 1,
            b')' if !in_squote && !in_dquote => depth_paren -= 1,
            b'[' if !in_squote && !in_dquote => depth_bracket += 1,
            b']' if !in_squote && !in_dquote => depth_bracket -= 1,
            b'{' if !in_squote && !in_dquote => depth_brace += 1,
            b'}' if !in_squote && !in_dquote => depth_brace -= 1,
            b',' if !in_squote
                && !in_dquote
                && depth_paren == 0
                && depth_bracket == 0
                && depth_brace == 0 =>
            {
                elements.push(inner[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
        prev = c;
    }
    if depth_paren != 0 || depth_bracket != 0 || depth_brace != 0 || in_squote || in_dquote {
        // Malformed — an unclosed bracket or string. Report
        // nothing rather than an arbitrary partial split; the
        // matching Datatype rule (or the parser's own error
        // path) will catch the structural problem first.
        return None;
    }
    elements.push(inner[start..].trim().to_string());
    Some(elements)
}

/// Parse a SET / inline literal as a numeric value for the
/// `Min/Max Inclusive` checks. Accepts both Int and Float wire
/// shapes (`42`, `3.14`); returns `None` for anything else
/// (string literal, parameter, function call, identifier, null).
/// String / param values would land here as well-formed quoted
/// or `$`-prefixed tokens, which we silently skip — the matching
/// Datatype constraint catches the type-axis violation separately.
pub(crate) fn parse_numeric_literal(raw: &str) -> Option<f64> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    let first = t.chars().next()?;
    let last = t.chars().next_back()?;
    // Quoted string / parameter / call / list / map → not numeric.
    if (first, last) == ('\'', '\'')
        || (first, last) == ('"', '"')
        || first == '$'
        || first == '['
        || first == '{'
        || t.contains('(')
    {
        return None;
    }
    if t.eq_ignore_ascii_case("null")
        || t.eq_ignore_ascii_case("true")
        || t.eq_ignore_ascii_case("false")
    {
        return None;
    }
    t.parse::<f64>().ok()
}

/// Whether a literal of `from` may legally assign to a slot of
/// `to`. Mirrors the standard SQL-ish widening you'd expect:
///
/// - Identical types are always compatible.
/// - `Int` assigns to `Float` (numeric widening).
/// - String literals assign to `Date`, `DateTime`, `Duration`,
///   `Bytes` — Cypher drivers parse the runtime string into the
///   target type, so the AST-time check stays narrow on purpose.
/// - Everything else is a mismatch.
///
/// The intentionally tight surface — strict on `Int → String` and
/// `String → Int` — catches the high-frequency LLM hallucination
/// (`SET p.age = 'twenty'`) without false-flagging the legitimate
/// `SET p.start_date = '2026-05-05'` shape.
pub(crate) fn type_assignable(from: &PropertyType, to: &PropertyType) -> bool {
    if from == to {
        return true;
    }
    match (from, to) {
        // Numeric widening.
        (PropertyType::Int, PropertyType::Float) => true,
        // String literals carry the runtime payload for these
        // types — Cypher drivers parse the string at write time.
        (
            PropertyType::String,
            PropertyType::Date
            | PropertyType::DateTime
            | PropertyType::Duration
            | PropertyType::Bytes,
        ) => true,
        _ => false,
    }
}

/// Strip surrounding single- or double-quotes and unescape embedded
/// quote characters. Returns `None` for anything that isn't a bare
/// string literal (parameters start with `$`, numerics are raw digits,
/// function calls carry parens). Keeping the extraction this strict
/// means we never produce a false positive by mis-interpreting
/// `$name` or `coalesce(a, b)` as the string literal `"$name"`.
fn parse_string_literal(raw: &str) -> Option<String> {
    let s = raw.trim();
    let (open, close) = s.chars().next().zip(s.chars().next_back())?;
    if (open, close) == ('"', '"') || (open, close) == ('\'', '\'') {
        let inner = &s[1..s.len() - 1];
        // Minimal unescape: `\"`, `\\`. Good enough for enum / regex
        // checks without pulling a full Cypher expression parser.
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some(esc) => out.push(esc),
                    None => return None,
                }
            } else {
                out.push(c);
            }
        }
        Some(out)
    } else {
        None
    }
}

fn property_by_id<'a>(node_type: &'a NodeTypeDef, id: &PropertyId) -> Option<&'a PropertyDef> {
    node_type.properties.iter().find(|p| p.id == *id)
}

/// Property-pair operator for `sh:lessThan` / `sh:equals` validation.
#[derive(Debug, Clone, Copy)]
enum PropertyPairOp {
    LessThan,
    Equals,
}

impl PropertyPairOp {
    fn diag_code(self) -> &'static str {
        match self {
            Self::LessThan => "runtime.cypher.shacl.less_than_violation",
            Self::Equals => "runtime.cypher.shacl.equals_violation",
        }
    }
}

/// AST-level enforcement of the property-pair constraints (`sh:lessThan`,
/// `sh:equals`). When the write pattern assigns both properties as
/// literals on the same node, evaluate the predicate. Skip when either
/// side is non-literal (parameter / function call) or when the constraint
/// references an unknown sibling property — rule-authoring smells get
/// surfaced as Info, real violations as the rule's severity.
fn check_property_pair_predicate(
    rule: &RuleDef,
    target_prop: &PropertyDef,
    node: &NodePattern,
    target: &ConstraintTarget,
    other_property: &PropertyId,
    op: PropertyPairOp,
    ontology: &ox_ontology::ir::OntologyIR,
    issues: &mut Vec<ValidationIssue>,
) {
    if !target_matches_property(target, target_prop) {
        return;
    }
    let Some(node_type) = ontology
        .node_types()
        .iter()
        .find(|nt| nt.properties.iter().any(|p| p.id == target_prop.id))
    else {
        return;
    };
    let Some(other_prop) = property_by_id(node_type, other_property) else {
        let rn = rule_label(rule);
        let key = target_prop.name.as_str();
        issues.push(ValidationIssue::info(
            "shacl",
            diag("runtime.cypher.shacl.property_pair_unknown_sibling")
                .with("property", key)
                .with("other_property_id", other_property.as_str())
                .with("rule_id", rule.id.as_str())
                .with("rule_name", &rule.name)
                .message(format!(
                    "rule `{rn}` references unknown sibling property `{}` for `{key}` and was skipped",
                    other_property.as_str()
                )),
        ));
        return;
    };
    let (Some(a_raw), Some(b_raw)) = (
        lookup_property_value(node, target_prop.name.as_str()),
        lookup_property_value(node, other_prop.name.as_str()),
    ) else {
        return;
    };
    let violated = compare_literals(a_raw, b_raw, op);
    let Some(true) = violated else {
        return;
    };
    let ak = target_prop.name.as_str();
    let bk = other_prop.name.as_str();
    let rn = rule_label(rule);
    let op_label = match op {
        PropertyPairOp::LessThan => "<",
        PropertyPairOp::Equals => "=",
    };
    issues.push(build_issue(
        rule,
        diag(op.diag_code())
            .with("property", ak)
            .with("other_property", bk)
            .with("value", a_raw)
            .with("other_value", b_raw)
            .with("rule_id", rule.id.as_str())
            .with("rule_name", &rule.name)
            .message(format!(
                "rule `{rn}` requires `{ak}` {op_label} `{bk}` but got {a_raw:?} vs {b_raw:?}"
            )),
        Some(node.span),
    ));
}

/// Compare two raw Cypher value tokens under the given operator.
/// Returns `Some(true)` when the predicate is *violated* (i.e. the
/// rule should produce a diagnostic), `Some(false)` when the predicate
/// holds, and `None` when at least one side is not statically
/// decidable (parameter, function call, opaque identifier).
///
/// Numeric literals are compared numerically; string literals
/// lexicographically. Mixed-type comparisons return `None` to avoid
/// false positives.
fn compare_literals(a_raw: &str, b_raw: &str, op: PropertyPairOp) -> Option<bool> {
    use std::cmp::Ordering;

    if let (Ok(a_num), Ok(b_num)) = (a_raw.trim().parse::<f64>(), b_raw.trim().parse::<f64>()) {
        // `partial_cmp` returns `None` for NaN — treat that as "cannot
        // decide statically" and skip the diagnostic rather than producing
        // a false positive.
        return a_num.partial_cmp(&b_num).map(|ord| match op {
            PropertyPairOp::LessThan => !matches!(ord, Ordering::Less),
            PropertyPairOp::Equals => !matches!(ord, Ordering::Equal),
        });
    }
    if let (Some(a), Some(b)) = (parse_string_literal(a_raw), parse_string_literal(b_raw)) {
        return Some(match op {
            PropertyPairOp::LessThan => a >= b,
            PropertyPairOp::Equals => a != b,
        });
    }
    None
}

fn build_issue(
    rule: &RuleDef,
    message: DiagnosticMessage,
    span: Option<crate::cypher::token::Span>,
) -> ValidationIssue {
    // Author-supplied `sh_message` rides alongside the catalogue
    // template — the FE / alerting pipeline reads `sh_message_default`
    // when present and falls back to the kind-keyed catalogue
    // otherwise. Wire shape stays additive so consumers without the
    // new field keep rendering the catalogue text unchanged.
    let mut final_message = message;
    if let Some(authored) = rule.sh_message.as_ref()
        && !authored.is_empty()
    {
        final_message.params.insert(
            "sh_message_default".to_string(),
            serde_json::Value::String(authored.default.clone()),
        );
        for (lang, text) in &authored.translations {
            final_message.params.insert(
                format!("sh_message_{}", lang.as_str()),
                serde_json::Value::String(text.clone()),
            );
        }
    }
    let issue = match rule.severity {
        Severity::Violation => ValidationIssue::error("shacl", final_message),
        Severity::Warning => ValidationIssue::warning("shacl", final_message),
        Severity::Info => ValidationIssue::info("shacl", final_message),
    };
    match span {
        Some(s) => issue.with_span(s),
        None => issue,
    }
}

/// Render a `RuleDef.name` for the English `message` rendering of a
/// SHACL diagnostic. Picks the rule's English translation when one
/// is authored (every derived rule is bilingual; authored rules may
/// be Korean-only) and falls back to the rule id when neither side
/// carries content. The structured `code` + `params` channel renders
/// the user-localised form independently.
fn rule_label(rule: &RuleDef) -> &str {
    let s = rule.name.english_or_default();
    if s.is_empty() {
        rule.id.as_str()
    } else {
        s
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use ox_core::i18n::LocalizedText;
    use ox_core::property_key::PropertyKey;
    use ox_ontology::action::RuleId;
    use ox_ontology::ir::{NodeTypeDef, NodeTypeId, OntologyIR, PropertyDef};
    use ox_ontology::rule::RuleOrigin;

    use crate::cypher::validate::IssueLevel;

    fn email_prop() -> PropertyDef {
        PropertyDef {
            id: PropertyId::new("prop-email"),
            name: PropertyKey::new("email").unwrap(),
            property_type: PropertyType::String,
            ..Default::default()
        }
    }

    fn user_with(prop: PropertyDef) -> NodeTypeDef {
        NodeTypeDef {
            id: NodeTypeId::new("nt-user"),
            label: GraphLabel::new("User").unwrap(),
            properties: vec![prop],
            ..Default::default()
        }
    }

    fn fixture_ontology_with_rule(rule: RuleDef, property: PropertyDef) -> OntologyIR {
        let mut onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![user_with(property)],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        onto.add_rule(rule).expect("rule add");
        onto
    }

    /// Build a code system + value set with the given codes, add
    /// both to the ontology, and return the `ValueSetId`.
    fn seed_value_set(onto: &mut OntologyIR, id: &str, codes: &[&str]) -> ox_ontology::ValueSetId {
        use ox_ontology::code_system::{CodeSystemDef, CodeSystemId, CodeSystemKind, CodedValue, CodedValueId};
        use ox_ontology::value_set::{
            IncludeMode, ValueSetDef, ValueSetId, ValueSetIncludeRule, ValueSetSelector,
        };
        let cs_id = CodeSystemId::new(format!("cs-{id}"));
        let vs_id = ValueSetId::new(format!("vs-{id}"));
        let cs = CodeSystemDef {
            id: cs_id.clone(),
            name: format!("cs-{id}"),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            uri: None,
            version: "1".into(),
            kind: CodeSystemKind::Internal,
            hierarchical: false,
            codes: codes
                .iter()
                .map(|c| CodedValue {
                    id: CodedValueId::new(format!("cv-{id}-{c}")),
                    code: (*c).to_string(),
                    display: LocalizedText::default(),
                    definition: LocalizedText::default(),
                    aliases: vec![],
                    broader_id: None,
                    examples: vec![],
                    scope_note: LocalizedText::default(),
                    valid_from: None,
                    valid_to: None,
                    deprecated_at: None,
                    replaced_by_id: None,
                })
                .collect(),
            deprecated_at: None,
            replaced_by_id: None,
        };
        onto.add_code_system(cs).expect("code system add");
        let vs = ValueSetDef {
            id: vs_id.clone(),
            name: format!("vs-{id}"),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            composition: vec![ValueSetIncludeRule {
                system_id: cs_id,
                selector: ValueSetSelector::All,
                mode: IncludeMode::Include,
            }],
        };
        onto.add_value_set(vs).expect("value set add");
        vs_id
    }

    /// Build a trivial `NotationPatternDef` from a single regex-like
    /// alphanumeric component. Sufficient for tests that only need
    /// "value matches / does not match" semantics.
    fn seed_alphanumeric_pattern(
        onto: &mut OntologyIR,
        id: &str,
        width: u32,
    ) -> ox_ontology::NotationPatternId {
        use ox_ontology::notation_pattern::{
            NotationComponent, NotationComponentKind, NotationPatternDef, NotationPatternId,
        };
        let np_id = NotationPatternId::new(format!("np-{id}"));
        let np = NotationPatternDef {
            id: np_id.clone(),
            name: format!("np-{id}"),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            template: format!("{{value:{width}}}"),
            separator: String::new(),
            components: vec![NotationComponent {
                name: "value".into(),
                display: LocalizedText::default(),
                kind: NotationComponentKind::Alphanumeric {
                    width,
                    uppercase: false,
                },
            }],
            examples: Vec::new(),
        };
        onto.add_notation_pattern(np).expect("notation pattern add");
        np_id
    }

    fn required_email_rule() -> (RuleDef, PropertyDef) {
        let prop = email_prop();
        let rule = RuleDef {
            id: RuleId::new("r-email-required"),
            name: "email_is_required".into(),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::PropertyShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
                target_property_id: PropertyId::new("prop-email"),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            origin: RuleOrigin::Authored,
            constraints: vec![ShaclConstraint::MinCount {
                target: ConstraintTarget::Inherit,
                min: 1,
            }],
                    valid_from: None,
            valid_to: None,
                    sh_message: None,
        };
        (rule, prop)
    }

    fn run(onto: OntologyIR, cypher: &str) -> Vec<ValidationIssue> {
        let ast = crate::cypher::parse(cypher);
        let v = ShaclValidator::new(onto);
        v.validate(&ast, &ValidateContext::new("ws-1"))
    }

    #[test]
    fn min_count_blocks_create_missing_property() {
        let (rule, prop) = required_email_rule();
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "CREATE (u:User {id: 1})");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.min_count_missing"
                && i.message.params.get("property")
                    == Some(&serde_json::Value::from("email"))),
            "{issues:?}"
        );
    }

    #[test]
    fn min_count_passes_create_with_property() {
        let (rule, prop) = required_email_rule();
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "CREATE (u:User {id: 1, email: 'a@b'})");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "{issues:?}"
        );
    }

    #[test]
    fn min_count_ignores_match() {
        // Rules default to Write enforcement — MATCH must not be checked.
        let (rule, prop) = required_email_rule();
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) RETURN u");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "{issues:?}"
        );
    }

    fn status_prop() -> PropertyDef {
        PropertyDef {
            id: PropertyId::new("prop-status"),
            name: PropertyKey::new("status").unwrap(),
            property_type: PropertyType::String,
            ..Default::default()
        }
    }

    fn enum_rule(vs_id: ox_ontology::ValueSetId) -> RuleDef {
        RuleDef {
            id: RuleId::new("r-status-enum"),
            name: "status_is_enumerated".into(),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::PropertyShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
                target_property_id: PropertyId::new("prop-status"),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            origin: RuleOrigin::Authored,
            constraints: vec![ShaclConstraint::InValueSet {
                target: ConstraintTarget::Inherit,
                value_set_id: vs_id,
            }],
            valid_from: None,
            valid_to: None,
                    sh_message: None,
        }
    }

    #[test]
    fn in_value_set_blocks_out_of_set_literal() {
        let prop = status_prop();
        let mut onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![user_with(prop)],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        let vs_id = seed_value_set(&mut onto, "status", &["active", "suspended"]);
        onto.add_rule(enum_rule(vs_id)).expect("rule add");
        let issues = run(onto, "CREATE (u:User {status: 'bogus'})");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.value_not_in_set"
                && i.message
                    .params
                    .get("value")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| s.contains("bogus"))),
            "{issues:?}"
        );
    }

    #[test]
    fn in_value_set_skips_parameter_bind() {
        // A `$param` value can't be decided at AST time — must not false-positive.
        let prop = status_prop();
        let mut onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![user_with(prop)],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        let vs_id = seed_value_set(&mut onto, "status", &["active"]);
        onto.add_rule(enum_rule(vs_id)).expect("rule add");
        let issues = run(onto, "CREATE (u:User {status: $s})");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "{issues:?}"
        );
    }

    #[test]
    fn in_value_set_blocks_unknown_numeric_literal() {
        // Status code as Int — `priority IN {1, 2, 3}`. A SET to
        // `99` must be rejected just like a string mismatch is.
        let prop = PropertyDef {
            id: PropertyId::new("prop-status"),
            name: PropertyKey::new("status").unwrap(),
            property_type: PropertyType::Int,
            ..Default::default()
        };
        let mut onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![user_with(prop)],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        let vs_id = seed_value_set(&mut onto, "status", &["1", "2", "3"]);
        onto.add_rule(enum_rule(vs_id)).expect("rule add");
        let issues = run(onto, "MATCH (u:User) SET u.status = 99 RETURN u");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.value_not_in_set"),
            "numeric literal not in value set must be flagged: {issues:?}"
        );
    }

    #[test]
    fn in_value_set_passes_known_numeric_literal() {
        let prop = PropertyDef {
            id: PropertyId::new("prop-status"),
            name: PropertyKey::new("status").unwrap(),
            property_type: PropertyType::Int,
            ..Default::default()
        };
        let mut onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![user_with(prop)],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        let vs_id = seed_value_set(&mut onto, "status", &["1", "2", "3"]);
        onto.add_rule(enum_rule(vs_id)).expect("rule add");
        let issues = run(onto, "MATCH (u:User) SET u.status = 2 RETURN u");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "numeric literal in value set must pass: {issues:?}"
        );
    }

    #[test]
    fn in_value_set_blocks_unknown_bool_literal() {
        // Bool-coded set — `enabled IN {true}`. Operator overrides
        // with `false` must be rejected.
        let prop = PropertyDef {
            id: PropertyId::new("prop-status"),
            name: PropertyKey::new("status").unwrap(),
            property_type: PropertyType::Bool,
            ..Default::default()
        };
        let mut onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![user_with(prop)],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        let vs_id = seed_value_set(&mut onto, "status", &["true"]);
        onto.add_rule(enum_rule(vs_id)).expect("rule add");
        let issues = run(onto, "MATCH (u:User) SET u.status = false RETURN u");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.value_not_in_set"),
            "bool literal not in value set must be flagged: {issues:?}"
        );
    }

    #[test]
    fn in_value_set_missing_reference_surfaces_as_info_not_error() {
        let prop = status_prop();
        let mut onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![user_with(prop)],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        // Deliberately do NOT seed the value set — rule references a
        // non-existent id. Runtime should log Info and skip the rule,
        // not block the write.
        let mut rule = enum_rule(ox_ontology::ValueSetId::new("vs-nope"));
        rule.severity = Severity::Violation;
        onto.add_rule(rule).expect("rule add");
        let issues = run(onto, "CREATE (u:User {status: 'anything'})");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "dangling value-set reference must not block writes: {issues:?}"
        );
    }

    #[test]
    fn matches_pattern_blocks_mismatched_literal() {
        let prop = email_prop();
        let mut onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![user_with(prop)],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        // A 5-character alphanumeric pattern — "abc" (3) and
        // "not-an-email" (12) both fail the fixed-width check.
        let np_id = seed_alphanumeric_pattern(&mut onto, "code5", 5);
        let rule = RuleDef {
            id: RuleId::new("r-code-pattern"),
            name: "code_format_is_valid".into(),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::PropertyShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
                target_property_id: PropertyId::new("prop-email"),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            origin: RuleOrigin::Authored,
            constraints: vec![ShaclConstraint::MatchesPattern {
                target: ConstraintTarget::Inherit,
                notation_pattern_id: np_id,
            }],
                    valid_from: None,
            valid_to: None,
                    sh_message: None,
        };
        onto.add_rule(rule).expect("rule add");
        let issues = run(onto, "CREATE (u:User {email: 'not-an-email'})");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.notation_pattern_mismatch"),
            "expected a NotationPattern violation: {issues:?}",
        );
    }

    #[test]
    fn severity_warning_maps_to_warning_issue() {
        let (mut rule, prop) = required_email_rule();
        rule.severity = Severity::Warning;
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "CREATE (u:User {id: 1})");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Warning
                && i.message.code == "runtime.cypher.shacl.min_count_missing"
                && i.message.params.get("property")
                    == Some(&serde_json::Value::from("email"))),
            "{issues:?}"
        );
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "{issues:?}"
        );
    }

    #[test]
    fn onaction_activation_is_skipped_at_pre_execute() {
        let (mut rule, prop) = required_email_rule();
        rule.activation = RuleActivationKind::OnAction {
            action_id: ox_ontology::action::ActionId::new("act-create-user"),
        };
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "CREATE (u:User {id: 1})");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "OnAction rules must not fire under the unconditional pre-execute pass: {issues:?}"
        );
    }

    #[test]
    fn read_enforcement_is_skipped_at_pre_execute() {
        let (mut rule, prop) = required_email_rule();
        rule.enforcement = EnforcementKind::Read;
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "CREATE (u:User {id: 1})");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "Read-time rules must not fire on Write pass: {issues:?}"
        );
    }

    // -------------------------------------------------------------
    // Closed / Disjoint / UniqueKey / UniqueLang constraints.
    // -------------------------------------------------------------

    fn closed_rule() -> RuleDef {
        RuleDef {
            id: RuleId::new("r-user-closed"),
            name: "user_closed_shape".into(),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::NodeShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            origin: RuleOrigin::Authored,
            constraints: vec![ShaclConstraint::Closed {
                target: ConstraintTarget::Inherit,
                // Only declared-on-shape keys beyond the node type's
                // own `email` property. So `email` is still allowed
                // (implicit), but `password` is not.
                allowed_properties: Vec::new(),
            }],
            valid_from: None,
            valid_to: None,
                    sh_message: None,
        }
    }

    #[test]
    fn closed_blocks_undeclared_property_on_create() {
        let prop = email_prop();
        let mut onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![user_with(prop)],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        onto.add_rule(closed_rule()).expect("rule add");
        let issues = run(onto, "CREATE (u:User {email: 'a@b', password: 'x'})");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.closed_shape_extra_property"
                && i.message.params.get("property")
                    == Some(&serde_json::Value::from("password"))),
            "closed shape must block undeclared `password`: {issues:?}"
        );
    }

    #[test]
    fn closed_allows_declared_properties() {
        let prop = email_prop();
        let mut onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![user_with(prop)],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        onto.add_rule(closed_rule()).expect("rule add");
        let issues = run(onto, "CREATE (u:User {email: 'a@b'})");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "closed shape must permit declared property: {issues:?}"
        );
    }

    #[test]
    fn disjoint_blocks_equal_literals_on_same_node() {
        let email = email_prop();
        let mut backup = email_prop();
        backup.id = PropertyId::new("prop-email-backup");
        backup.name = PropertyKey::new("email_backup").unwrap();
        let node = NodeTypeDef {
            id: NodeTypeId::new("nt-user"),
            label: GraphLabel::new("User").unwrap(),
            properties: vec![email, backup],
            ..Default::default()
        };
        let mut onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![node],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        onto.add_rule(RuleDef {
            id: RuleId::new("r-disjoint"),
            name: "email_vs_backup_disjoint".into(),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::NodeShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            origin: RuleOrigin::Authored,
            constraints: vec![ShaclConstraint::Disjoint {
                a: ConstraintTarget::Property {
                    node_type_id: NodeTypeId::new("nt-user"),
                    property_id: PropertyId::new("prop-email"),
                },
                b: ConstraintTarget::Property {
                    node_type_id: NodeTypeId::new("nt-user"),
                    property_id: PropertyId::new("prop-email-backup"),
                },
            }],
                    valid_from: None,
            valid_to: None,
                    sh_message: None,
        })
        .expect("rule add");
        let issues = run(
            onto,
            "CREATE (u:User {email: 'a@b', email_backup: 'a@b'})",
        );
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.disjoint_pair_present"),
            "disjoint violation not surfaced: {issues:?}"
        );
    }

    #[test]
    fn disjoint_passes_when_values_differ() {
        let email = email_prop();
        let mut backup = email_prop();
        backup.id = PropertyId::new("prop-email-backup");
        backup.name = PropertyKey::new("email_backup").unwrap();
        let node = NodeTypeDef {
            id: NodeTypeId::new("nt-user"),
            label: GraphLabel::new("User").unwrap(),
            properties: vec![email, backup],
            ..Default::default()
        };
        let mut onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![node],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        onto.add_rule(RuleDef {
            id: RuleId::new("r-disjoint"),
            name: "email_vs_backup_disjoint".into(),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::NodeShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            origin: RuleOrigin::Authored,
            constraints: vec![ShaclConstraint::Disjoint {
                a: ConstraintTarget::Property {
                    node_type_id: NodeTypeId::new("nt-user"),
                    property_id: PropertyId::new("prop-email"),
                },
                b: ConstraintTarget::Property {
                    node_type_id: NodeTypeId::new("nt-user"),
                    property_id: PropertyId::new("prop-email-backup"),
                },
            }],
                    valid_from: None,
            valid_to: None,
                    sh_message: None,
        })
        .expect("rule add");
        let issues = run(
            onto,
            "CREATE (u:User {email: 'a@b', email_backup: 'c@d'})",
        );
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "disjoint must pass when values differ: {issues:?}"
        );
    }

    #[test]
    fn unique_key_rule_surfaces_missing_property() {
        let prop = email_prop();
        let mut onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![user_with(prop)],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        onto.add_rule(RuleDef {
            id: RuleId::new("r-unique"),
            name: "composite_unique".into(),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::NodeShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            origin: RuleOrigin::Authored,
            constraints: vec![ShaclConstraint::UniqueKey {
                target_node_type_id: NodeTypeId::new("nt-user"),
                property_keys: vec![PropertyKey::new("email").unwrap(), PropertyKey::new("tenant").unwrap()],
            }],
                    valid_from: None,
            valid_to: None,
                    sh_message: None,
        })
        .expect("rule add");
        let issues = run(onto, "CREATE (u:User {email: 'a@b'})");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.unique_key_unknown_property"
                && i.message.params.get("property")
                    == Some(&serde_json::Value::from("tenant"))),
            "unique-key rule referencing unknown `tenant` property must surface: {issues:?}"
        );
    }

    #[test]
    fn unique_lang_on_non_localized_property_surfaces_info() {
        // `email` is not declared is_localized, so UniqueLang is a
        // rule-authoring smell — surface as Info.
        let prop = email_prop();
        let mut onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![user_with(prop)],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        onto.add_rule(RuleDef {
            id: RuleId::new("r-ulang"),
            name: "email_unique_lang".into(),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::PropertyShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
                target_property_id: PropertyId::new("prop-email"),
            },
            severity: Severity::Warning,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            origin: RuleOrigin::Authored,
            constraints: vec![ShaclConstraint::UniqueLang {
                target: ConstraintTarget::Property {
                    node_type_id: NodeTypeId::new("nt-user"),
                    property_id: PropertyId::new("prop-email"),
                },
            }],
                    valid_from: None,
            valid_to: None,
                    sh_message: None,
        })
        .expect("rule add");
        let issues = run(onto, "CREATE (u:User {email: 'a@b'})");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Info
                && i.message.code == "runtime.cypher.shacl.unique_lang_on_non_localized"),
            "non-localized UniqueLang must surface Info: {issues:?}"
        );
    }

    // -------------------------------------------------------------
    // Wave 8.7 — `Required` PropertyBinding becomes a SHACL rule
    // automatically; the validator enforces without an authored rule.
    // -------------------------------------------------------------

    #[test]
    fn required_value_set_binding_blocks_out_of_set_literal_without_authored_rule() {
        use ox_ontology::binding::{BindingStrength, PropertyBinding};

        let prop = status_prop();
        let mut onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![user_with(prop)],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        let vs_id = seed_value_set(&mut onto, "status", &["active", "suspended"]);

        // Required binding — no authored rule. The derived rule must
        // pick this up and reject the out-of-set literal.
        onto.node_types_mut()
            .iter_mut()
            .next()
            .unwrap()
            .properties[0]
            .bindings
            .push(
                PropertyBinding::value_set(vs_id)
                    .with_strength(BindingStrength::Required),
            );
        onto.rebuild_indices().expect("rebuild");

        let issues = run(onto, "CREATE (u:User {status: 'bogus'})");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.value_not_in_set"
                && i.message
                    .params
                    .get("value")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| s.contains("bogus"))),
            "Required ValueSet binding must reject out-of-set literal: {issues:?}"
        );
    }

    #[test]
    fn preferred_binding_does_not_synthesise_blocking_rule() {
        use ox_ontology::binding::{BindingStrength, PropertyBinding};

        let prop = status_prop();
        let mut onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![user_with(prop)],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        let vs_id = seed_value_set(&mut onto, "status", &["active"]);

        // Preferred — guidance only, no enforcement.
        onto.node_types_mut()
            .iter_mut()
            .next()
            .unwrap()
            .properties[0]
            .bindings
            .push(
                PropertyBinding::value_set(vs_id)
                    .with_strength(BindingStrength::Preferred),
            );
        onto.rebuild_indices().expect("rebuild");

        let issues = run(onto, "CREATE (u:User {status: 'bogus'})");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "Preferred binding must not block writes: {issues:?}"
        );
    }

    // ---------- SET / REMOVE coverage ----------

    #[test]
    fn set_blocks_out_of_value_set_literal() {
        let prop = status_prop();
        let mut onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![user_with(prop)],
            vec![],
            vec![],
        )
        .expect("seed");
        let vs_id = seed_value_set(&mut onto, "status", &["A", "B"]);
        onto.add_rule(enum_rule(vs_id)).expect("rule");

        let issues = run(onto, "MATCH (u:User) SET u.status = 'C'");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.value_not_in_set"
                && i.message.params.get("property")
                    == Some(&serde_json::Value::from("status"))),
            "SET of out-of-set literal must fire value_not_in_set: {issues:?}"
        );
    }

    #[test]
    fn set_passes_in_value_set_literal() {
        let prop = status_prop();
        let mut onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![user_with(prop)],
            vec![],
            vec![],
        )
        .expect("seed");
        let vs_id = seed_value_set(&mut onto, "status", &["A", "B"]);
        onto.add_rule(enum_rule(vs_id)).expect("rule");

        let issues = run(onto, "MATCH (u:User) SET u.status = 'A'");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "in-set SET must not block: {issues:?}"
        );
    }

    #[test]
    fn set_skips_parameter_value() {
        let prop = status_prop();
        let mut onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![user_with(prop)],
            vec![],
            vec![],
        )
        .expect("seed");
        let vs_id = seed_value_set(&mut onto, "status", &["A", "B"]);
        onto.add_rule(enum_rule(vs_id)).expect("rule");

        let issues = run(onto, "MATCH (u:User) SET u.status = $new_status");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "non-literal SET value can't be settled at AST time: {issues:?}"
        );
    }

    #[test]
    fn set_against_unbound_variable_skips_silently() {
        // Variable not introduced by any MATCH / CREATE / MERGE in
        // this statement — schema conformance is not the SHACL
        // validator's job, so the absence of label resolution must
        // produce no false positive (no panic, no error).
        let prop = status_prop();
        let mut onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![user_with(prop)],
            vec![],
            vec![],
        )
        .expect("seed");
        let vs_id = seed_value_set(&mut onto, "status", &["A", "B"]);
        onto.add_rule(enum_rule(vs_id)).expect("rule");

        let issues = run(onto, "WITH 1 AS x SET x.status = 'C'");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "unbound variable SET must not raise SHACL errors: {issues:?}"
        );
    }

    #[test]
    fn remove_blocks_required_property() {
        let (rule, prop) = required_email_rule();
        let onto = fixture_ontology_with_rule(rule, prop);

        let issues = run(onto, "MATCH (u:User) REMOVE u.email");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.min_count_removed"
                && i.message.params.get("property")
                    == Some(&serde_json::Value::from("email"))),
            "REMOVE of required property must fire min_count_removed: {issues:?}"
        );
    }

    #[test]
    fn remove_optional_property_is_silent() {
        let prop = status_prop();
        let onto = fixture_ontology_with_rule(
            RuleDef {
                id: RuleId::new("r-status-min0"),
                name: "status_optional".into(),
                description: LocalizedText::default(),
                rationale: LocalizedText::default(),
                kind: RuleKind::PropertyShape {
                    target_node_type_id: NodeTypeId::new("nt-user"),
                    target_property_id: PropertyId::new("prop-status"),
                },
                severity: Severity::Violation,
                enforcement: EnforcementKind::Write,
                activation: RuleActivationKind::Always,
                origin: RuleOrigin::Authored,
                constraints: vec![ShaclConstraint::MinCount {
                    target: ConstraintTarget::Inherit,
                    min: 0,
                }],
                valid_from: None,
                valid_to: None,
                sh_message: None,
            },
            prop,
        );

        let issues = run(onto, "MATCH (u:User) REMOVE u.status");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "REMOVE of optional property must not fire: {issues:?}"
        );
    }

    // ---- Datatype enforcement -----------------------------------------

    fn age_prop() -> PropertyDef {
        PropertyDef {
            id: PropertyId::new("prop-age"),
            name: PropertyKey::new("age").unwrap(),
            property_type: PropertyType::Int,
            ..Default::default()
        }
    }

    fn datatype_rule(target_property_id: &str, expected: PropertyType) -> RuleDef {
        RuleDef {
            id: RuleId::new(format!("r-datatype-{target_property_id}")),
            name: format!("{target_property_id}_datatype").into(),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::PropertyShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
                target_property_id: PropertyId::new(target_property_id),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            origin: RuleOrigin::Authored,
            constraints: vec![ShaclConstraint::Datatype {
                target: ConstraintTarget::Inherit,
                expected,
            }],
            valid_from: None,
            valid_to: None,
            sh_message: None,
        }
    }

    #[test]
    fn datatype_blocks_set_string_into_int_slot() {
        // `SET u.age = 'twenty'` — common LLM hallucination shape.
        // The driver would silently store the string at runtime;
        // validator catches it pre-execute.
        let prop = age_prop();
        let rule = datatype_rule("prop-age", PropertyType::Int);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.age = 'twenty' RETURN u");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.datatype_mismatch"
                && i.message.params.get("expected_type")
                    == Some(&serde_json::Value::from("int"))
                && i.message.params.get("observed_type")
                    == Some(&serde_json::Value::from("string"))),
            "expected datatype_mismatch on Int ← String: {issues:?}"
        );
    }

    #[test]
    fn datatype_passes_set_int_into_int_slot() {
        let prop = age_prop();
        let rule = datatype_rule("prop-age", PropertyType::Int);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.age = 42 RETURN u");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "Int literal into Int slot must pass: {issues:?}"
        );
    }

    #[test]
    fn datatype_passes_set_int_into_float_slot_via_widening() {
        // SQL-ish numeric widening — `SET u.score = 1` on a Float
        // property is the legit shape, must not false-flag.
        let prop = PropertyDef {
            id: PropertyId::new("prop-score"),
            name: PropertyKey::new("score").unwrap(),
            property_type: PropertyType::Float,
            ..Default::default()
        };
        let rule = datatype_rule("prop-score", PropertyType::Float);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.score = 1 RETURN u");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "Int → Float widening must pass: {issues:?}"
        );
    }

    #[test]
    fn datatype_passes_set_string_into_date_slot() {
        // ISO date strings are how Cypher drivers receive Date /
        // DateTime / Duration values — must not be flagged.
        let prop = PropertyDef {
            id: PropertyId::new("prop-start"),
            name: PropertyKey::new("start").unwrap(),
            property_type: PropertyType::Date,
            ..Default::default()
        };
        let rule = datatype_rule("prop-start", PropertyType::Date);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.start = '2026-05-05' RETURN u");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "String → Date must pass: {issues:?}"
        );
    }

    #[test]
    fn datatype_blocks_create_with_inline_string_for_int_slot() {
        // Same check applies to inline `(n:User {age: 'old'})` —
        // the validator goes through the same per-property pass
        // for CREATE clauses, and Datatype constraint must be
        // evaluated there too.
        let prop = age_prop();
        let rule = datatype_rule("prop-age", PropertyType::Int);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "CREATE (u:User {age: 'old'})");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.datatype_mismatch"),
            "expected datatype_mismatch on inline CREATE: {issues:?}"
        );
    }

    #[test]
    fn datatype_skips_set_when_value_is_a_parameter() {
        // `SET u.age = $age` — cannot decide at AST time. Driver
        // takes the call. The validator must not false-flag.
        let prop = age_prop();
        let rule = datatype_rule("prop-age", PropertyType::Int);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.age = $age RETURN u");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "$param assignment must not raise datatype_mismatch: {issues:?}"
        );
    }

    #[test]
    fn datatype_skips_set_when_value_is_null() {
        // null means "absence of value", not a typed value.
        // MinCount / nullable rules elsewhere police that axis.
        let prop = age_prop();
        let rule = datatype_rule("prop-age", PropertyType::Int);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.age = null RETURN u");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "null assignment must not raise datatype_mismatch: {issues:?}"
        );
    }

    #[test]
    fn datatype_blocks_set_string_into_bool_slot() {
        let prop = PropertyDef {
            id: PropertyId::new("prop-active"),
            name: PropertyKey::new("active").unwrap(),
            property_type: PropertyType::Bool,
            ..Default::default()
        };
        let rule = datatype_rule("prop-active", PropertyType::Bool);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.active = 'yes' RETURN u");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.datatype_mismatch"
                && i.message.params.get("expected_type")
                    == Some(&serde_json::Value::from("bool"))),
            "expected datatype_mismatch on Bool ← String: {issues:?}"
        );
    }

    #[test]
    fn classify_literal_type_recognises_each_kind() {
        assert_eq!(classify_literal_type("'hello'"), Some(PropertyType::String));
        assert_eq!(classify_literal_type("\"hello\""), Some(PropertyType::String));
        assert_eq!(classify_literal_type("42"), Some(PropertyType::Int));
        assert_eq!(classify_literal_type("3.14"), Some(PropertyType::Float));
        assert_eq!(classify_literal_type("true"), Some(PropertyType::Bool));
        assert_eq!(classify_literal_type("FALSE"), Some(PropertyType::Bool));
        assert_eq!(classify_literal_type("null"), None);
        assert_eq!(classify_literal_type("$age"), None);
        assert_eq!(classify_literal_type("coalesce(a, 1)"), None);
        assert_eq!(classify_literal_type("n.other"), None);
        assert_eq!(classify_literal_type("[1, 2, 3]"), None);
        assert_eq!(classify_literal_type("{ a: 1 }"), None);
        assert_eq!(classify_literal_type(""), None);
    }

    // ---- MinInclusive / MaxInclusive enforcement -----------------------

    fn score_prop() -> PropertyDef {
        PropertyDef {
            id: PropertyId::new("prop-score"),
            name: PropertyKey::new("score").unwrap(),
            property_type: PropertyType::Float,
            ..Default::default()
        }
    }

    fn min_inclusive_rule(target_property_id: &str, min: f64) -> RuleDef {
        RuleDef {
            id: RuleId::new(format!("r-min-{target_property_id}")),
            name: format!("{target_property_id}_min").into(),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::PropertyShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
                target_property_id: PropertyId::new(target_property_id),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            origin: RuleOrigin::Authored,
            constraints: vec![ShaclConstraint::MinInclusive {
                target: ConstraintTarget::Inherit,
                min,
            }],
            valid_from: None,
            valid_to: None,
            sh_message: None,
        }
    }

    fn max_inclusive_rule(target_property_id: &str, max: f64) -> RuleDef {
        RuleDef {
            id: RuleId::new(format!("r-max-{target_property_id}")),
            name: format!("{target_property_id}_max").into(),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::PropertyShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
                target_property_id: PropertyId::new(target_property_id),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            origin: RuleOrigin::Authored,
            constraints: vec![ShaclConstraint::MaxInclusive {
                target: ConstraintTarget::Inherit,
                max,
            }],
            valid_from: None,
            valid_to: None,
            sh_message: None,
        }
    }

    #[test]
    fn min_inclusive_blocks_value_below_min() {
        let prop = score_prop();
        let rule = min_inclusive_rule("prop-score", 0.0);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.score = -1 RETURN u");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.min_inclusive_violation"),
            "expected min_inclusive violation: {issues:?}"
        );
    }

    #[test]
    fn min_inclusive_passes_at_boundary() {
        let prop = score_prop();
        let rule = min_inclusive_rule("prop-score", 0.0);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.score = 0 RETURN u");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "boundary value `min` must pass (inclusive): {issues:?}"
        );
    }

    #[test]
    fn max_inclusive_blocks_value_above_max() {
        let prop = score_prop();
        let rule = max_inclusive_rule("prop-score", 100.0);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.score = 101 RETURN u");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.max_inclusive_violation"),
            "expected max_inclusive violation: {issues:?}"
        );
    }

    #[test]
    fn max_inclusive_passes_at_boundary() {
        let prop = score_prop();
        let rule = max_inclusive_rule("prop-score", 100.0);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.score = 100 RETURN u");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "boundary value `max` must pass (inclusive): {issues:?}"
        );
    }

    #[test]
    fn min_inclusive_skips_non_numeric_literal() {
        // String / param: cannot decide numerically. The matching
        // Datatype rule (if any) would catch the type axis; this
        // pass must silently skip rather than false-flag.
        let prop = score_prop();
        let rule = min_inclusive_rule("prop-score", 0.0);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.score = $score RETURN u");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "$param assignment must not raise min_inclusive: {issues:?}"
        );
    }

    #[test]
    fn min_inclusive_blocks_inline_create_value() {
        let prop = score_prop();
        let rule = min_inclusive_rule("prop-score", 0.0);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "CREATE (u:User {score: -5})");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.min_inclusive_violation"),
            "expected min_inclusive on inline CREATE: {issues:?}"
        );
    }

    // ---- MinLength / MaxLength enforcement -----------------------------

    fn email_prop_for_length() -> PropertyDef {
        PropertyDef {
            id: PropertyId::new("prop-email"),
            name: PropertyKey::new("email").unwrap(),
            property_type: PropertyType::String,
            ..Default::default()
        }
    }

    fn min_length_rule(min: u32) -> RuleDef {
        RuleDef {
            id: RuleId::new("r-email-minlen"),
            name: "email_minlen".into(),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::PropertyShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
                target_property_id: PropertyId::new("prop-email"),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            origin: RuleOrigin::Authored,
            constraints: vec![ShaclConstraint::MinLength {
                target: ConstraintTarget::Inherit,
                min,
            }],
            valid_from: None,
            valid_to: None,
            sh_message: None,
        }
    }

    fn max_length_rule(max: u32) -> RuleDef {
        RuleDef {
            id: RuleId::new("r-email-maxlen"),
            name: "email_maxlen".into(),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::PropertyShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
                target_property_id: PropertyId::new("prop-email"),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            origin: RuleOrigin::Authored,
            constraints: vec![ShaclConstraint::MaxLength {
                target: ConstraintTarget::Inherit,
                max,
            }],
            valid_from: None,
            valid_to: None,
            sh_message: None,
        }
    }

    #[test]
    fn min_length_blocks_short_string() {
        let prop = email_prop_for_length();
        let rule = min_length_rule(5);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "CREATE (u:User {email: 'a@b'})");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.min_length_violation"
                && i.message.params.get("observed")
                    == Some(&serde_json::Value::from(3u64))),
            "expected min_length violation with observed=3: {issues:?}"
        );
    }

    #[test]
    fn min_length_passes_at_boundary() {
        let prop = email_prop_for_length();
        let rule = min_length_rule(3);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "CREATE (u:User {email: 'a@b'})");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "boundary length 3 must pass: {issues:?}"
        );
    }

    #[test]
    fn max_length_blocks_long_string() {
        let prop = email_prop_for_length();
        let rule = max_length_rule(5);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "CREATE (u:User {email: 'a@b.com'})");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.max_length_violation"
                && i.message.params.get("observed")
                    == Some(&serde_json::Value::from(7u64))),
            "expected max_length violation with observed=7: {issues:?}"
        );
    }

    #[test]
    fn length_check_counts_unicode_codepoints_not_bytes() {
        // 한글 char 3개 = 3 codepoints, but 9 UTF-8 bytes. The
        // length axis is "what users perceive" — codepoints, not
        // raw byte count.
        let prop = email_prop_for_length();
        let rule = max_length_rule(3);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "CREATE (u:User {email: '한글짧'})");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "3-codepoint Korean string must pass max_length=3: {issues:?}"
        );
    }

    #[test]
    fn length_check_skips_non_string_literal() {
        // SET u.email = 42 — not a string literal. parse_string_literal
        // returns None, so the length check silently skips. Datatype
        // rule (if any) catches the type-axis violation separately.
        let prop = email_prop_for_length();
        let rule = min_length_rule(5);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.email = 42 RETURN u");
        assert!(
            issues
                .iter()
                .all(|i| i.message.code != "runtime.cypher.shacl.min_length_violation"),
            "non-string literal must not raise min_length: {issues:?}"
        );
    }

    // ---- Nullable=false implicit derivation ----------------------------

    /// CREATE without the nullable=false property must be flagged
    /// even when the operator did NOT author an explicit MinCount=1
    /// rule. The schema-level NOT NULL declaration is the source of
    /// truth; the platform safety net derives the equivalent SHACL
    /// rule.
    #[test]
    fn nullable_false_property_required_at_create() {
        let prop = PropertyDef {
            id: PropertyId::new("prop-name"),
            name: PropertyKey::new("name").unwrap(),
            property_type: PropertyType::String,
            nullable: false,
            ..Default::default()
        };
        let onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![user_with(prop)],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        let issues = run(onto, "CREATE (u:User {id: 1})");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.min_count_missing"),
            "nullable=false derivation must surface as min_count_missing: {issues:?}"
        );
    }

    /// nullable=true property may legitimately be omitted from a
    /// CREATE — derivation must NOT fire.
    #[test]
    fn nullable_true_property_optional_at_create() {
        let prop = PropertyDef {
            id: PropertyId::new("prop-bio"),
            name: PropertyKey::new("bio").unwrap(),
            property_type: PropertyType::String,
            nullable: true,
            ..Default::default()
        };
        let onto = OntologyIR::try_new(
            "ont-test".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![user_with(prop)],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        let issues = run(onto, "CREATE (u:User {id: 1})");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "nullable=true must not raise min_count_missing: {issues:?}"
        );
    }

    // ---- HasValue enforcement -----------------------------------------

    fn has_value_rule(target_property_id: &str, expected: &str) -> RuleDef {
        RuleDef {
            id: RuleId::new(format!("r-hasvalue-{target_property_id}")),
            name: format!("{target_property_id}_hasvalue").into(),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::PropertyShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
                target_property_id: PropertyId::new(target_property_id),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            origin: RuleOrigin::Authored,
            constraints: vec![ShaclConstraint::HasValue {
                target: ConstraintTarget::Inherit,
                value: expected.to_string(),
            }],
            valid_from: None,
            valid_to: None,
            sh_message: None,
        }
    }

    #[test]
    fn has_value_blocks_mismatched_string_literal() {
        let prop = status_prop();
        let rule = has_value_rule("prop-status", "active");
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.status = 'inactive' RETURN u");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.has_value_violation"
                && i.message.params.get("expected")
                    == Some(&serde_json::Value::from("active"))),
            "expected has_value violation: {issues:?}"
        );
    }

    #[test]
    fn has_value_passes_matching_string_literal() {
        let prop = status_prop();
        let rule = has_value_rule("prop-status", "active");
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.status = 'active' RETURN u");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "matching string literal must pass: {issues:?}"
        );
    }

    #[test]
    fn has_value_blocks_mismatched_numeric_literal() {
        let prop = PropertyDef {
            id: PropertyId::new("prop-version"),
            name: PropertyKey::new("version").unwrap(),
            property_type: PropertyType::Int,
            ..Default::default()
        };
        let rule = has_value_rule("prop-version", "1");
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.version = 2 RETURN u");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.has_value_violation"),
            "expected has_value violation on numeric: {issues:?}"
        );
    }

    #[test]
    fn has_value_passes_matching_numeric_literal() {
        let prop = PropertyDef {
            id: PropertyId::new("prop-version"),
            name: PropertyKey::new("version").unwrap(),
            property_type: PropertyType::Int,
            ..Default::default()
        };
        let rule = has_value_rule("prop-version", "1");
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.version = 1 RETURN u");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "matching numeric literal must pass: {issues:?}"
        );
    }

    #[test]
    fn has_value_passes_matching_bool_literal() {
        let prop = PropertyDef {
            id: PropertyId::new("prop-flag"),
            name: PropertyKey::new("flag").unwrap(),
            property_type: PropertyType::Bool,
            ..Default::default()
        };
        let rule = has_value_rule("prop-flag", "true");
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.flag = true RETURN u");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "matching bool literal must pass: {issues:?}"
        );
    }

    #[test]
    fn has_value_skips_parameter_assignment() {
        let prop = status_prop();
        let rule = has_value_rule("prop-status", "active");
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.status = $st RETURN u");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "$param must not raise has_value: {issues:?}"
        );
    }

    #[test]
    fn has_value_blocks_inline_create_value() {
        let prop = status_prop();
        let rule = has_value_rule("prop-status", "active");
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "CREATE (u:User {status: 'archived'})");
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.has_value_violation"),
            "expected has_value violation on inline CREATE: {issues:?}"
        );
    }

    // ---- MaxCount enforcement ------------------------------------------

    fn tags_prop() -> PropertyDef {
        PropertyDef {
            id: PropertyId::new("prop-tags"),
            name: PropertyKey::new("tags").unwrap(),
            property_type: PropertyType::List {
                element: Box::new(PropertyType::String),
            },
            ..Default::default()
        }
    }

    fn max_count_rule(target_property_id: &str, max: u32) -> RuleDef {
        RuleDef {
            id: RuleId::new(format!("r-maxc-{target_property_id}")),
            name: format!("{target_property_id}_maxcount").into(),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::PropertyShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
                target_property_id: PropertyId::new(target_property_id),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            origin: RuleOrigin::Authored,
            constraints: vec![ShaclConstraint::MaxCount {
                target: ConstraintTarget::Inherit,
                max,
            }],
            valid_from: None,
            valid_to: None,
            sh_message: None,
        }
    }

    #[test]
    fn max_count_blocks_oversized_list_literal() {
        let prop = tags_prop();
        let rule = max_count_rule("prop-tags", 3);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(
            onto,
            "MATCH (u:User) SET u.tags = ['a', 'b', 'c', 'd'] RETURN u",
        );
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.max_count_violation"
                && i.message.params.get("observed")
                    == Some(&serde_json::Value::from(4u64))),
            "expected max_count_violation with observed=4: {issues:?}"
        );
    }

    #[test]
    fn max_count_passes_at_boundary() {
        let prop = tags_prop();
        let rule = max_count_rule("prop-tags", 3);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.tags = ['a', 'b', 'c'] RETURN u");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "boundary count must pass: {issues:?}"
        );
    }

    #[test]
    fn max_count_passes_empty_list() {
        let prop = tags_prop();
        let rule = max_count_rule("prop-tags", 3);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.tags = [] RETURN u");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "empty list must pass: {issues:?}"
        );
    }

    #[test]
    fn max_count_skips_non_list_literal() {
        // Single-value SET — the AST cannot decide cross-write
        // multiplicity, and the matching Datatype rule (if any)
        // catches the type-axis mismatch instead.
        let prop = tags_prop();
        let rule = max_count_rule("prop-tags", 3);
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.tags = $tags RETURN u");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "$param must not raise max_count: {issues:?}"
        );
    }

    // ---- Datatype List element-wise enforcement ------------------------

    fn datatype_list_string_rule() -> RuleDef {
        RuleDef {
            id: RuleId::new("r-tags-datatype-list-string"),
            name: "tags_list_of_string".into(),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::PropertyShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
                target_property_id: PropertyId::new("prop-tags"),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            origin: RuleOrigin::Authored,
            constraints: vec![ShaclConstraint::Datatype {
                target: ConstraintTarget::Inherit,
                expected: PropertyType::List {
                    element: Box::new(PropertyType::String),
                },
            }],
            valid_from: None,
            valid_to: None,
            sh_message: None,
        }
    }

    #[test]
    fn datatype_list_blocks_mismatched_element_type() {
        let prop = tags_prop();
        let rule = datatype_list_string_rule();
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(
            onto,
            "MATCH (u:User) SET u.tags = ['a', 1, 'c'] RETURN u",
        );
        assert!(
            issues.iter().any(|i| i.level == IssueLevel::Error
                && i.message.code == "runtime.cypher.shacl.list_element_datatype_mismatch"
                && i.message.params.get("element_index")
                    == Some(&serde_json::Value::from(1u64))
                && i.message.params.get("expected_type")
                    == Some(&serde_json::Value::from("string"))
                && i.message.params.get("observed_type")
                    == Some(&serde_json::Value::from("int"))),
            "expected list_element_datatype_mismatch on Int element: {issues:?}"
        );
    }

    #[test]
    fn datatype_list_passes_uniform_string_elements() {
        let prop = tags_prop();
        let rule = datatype_list_string_rule();
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "MATCH (u:User) SET u.tags = ['a', 'b', 'c'] RETURN u");
        assert!(
            issues.iter().all(|i| i.level != IssueLevel::Error),
            "uniform string list must pass: {issues:?}"
        );
    }

    #[test]
    fn datatype_list_emits_one_issue_per_offending_element() {
        let prop = tags_prop();
        let rule = datatype_list_string_rule();
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(
            onto,
            "MATCH (u:User) SET u.tags = ['a', 1, 'b', 2] RETURN u",
        );
        let count = issues
            .iter()
            .filter(|i| {
                i.level == IssueLevel::Error
                    && i.message.code == "runtime.cypher.shacl.list_element_datatype_mismatch"
            })
            .count();
        assert_eq!(count, 2, "expected one issue per non-string element: {issues:?}");
    }

    #[test]
    fn parse_list_literal_handles_nested_and_quoted_commas() {
        assert_eq!(parse_list_literal("[]"), Some(Vec::<String>::new()));
        assert_eq!(
            parse_list_literal("[1, 2, 3]"),
            Some(vec!["1".to_string(), "2".to_string(), "3".to_string()])
        );
        assert_eq!(
            parse_list_literal("['a, b', 'c']"),
            Some(vec!["'a, b'".to_string(), "'c'".to_string()])
        );
        assert_eq!(
            parse_list_literal("[[1, 2], [3, 4]]"),
            Some(vec!["[1, 2]".to_string(), "[3, 4]".to_string()])
        );
        assert_eq!(parse_list_literal("[1]"), Some(vec!["1".to_string()]));
        assert_eq!(parse_list_literal("'foo'"), None);
        assert_eq!(parse_list_literal("[unclosed"), None);
        assert_eq!(parse_list_literal("[1, 'unclosed]"), None);
    }

    #[test]
    fn parse_numeric_literal_recognises_int_and_float() {
        assert_eq!(parse_numeric_literal("42"), Some(42.0));
        assert_eq!(parse_numeric_literal("-1"), Some(-1.0));
        assert_eq!(parse_numeric_literal("2.5"), Some(2.5));
        assert_eq!(parse_numeric_literal("0"), Some(0.0));
        assert_eq!(parse_numeric_literal("'42'"), None);
        assert_eq!(parse_numeric_literal("$x"), None);
        assert_eq!(parse_numeric_literal("null"), None);
        assert_eq!(parse_numeric_literal("true"), None);
        assert_eq!(parse_numeric_literal("toString(n)"), None);
        assert_eq!(parse_numeric_literal("[1,2]"), None);
    }

    #[test]
    fn type_assignable_widening_matrix() {
        use PropertyType::*;
        // Same kind always assignable.
        assert!(type_assignable(&Int, &Int));
        assert!(type_assignable(&String, &String));
        // Numeric widening Int → Float.
        assert!(type_assignable(&Int, &Float));
        // Float → Int is NOT assignable (precision loss).
        assert!(!type_assignable(&Float, &Int));
        // String literals carry runtime payload for temporal /
        // bytes types.
        assert!(type_assignable(&String, &Date));
        assert!(type_assignable(&String, &DateTime));
        assert!(type_assignable(&String, &Duration));
        assert!(type_assignable(&String, &Bytes));
        // Int → String / Bool → String / etc. are NOT auto-assignable
        // in this conservative model — explicit `toString` cast is
        // expected and the SET/CREATE clause would carry the
        // resulting String literal anyway.
        assert!(!type_assignable(&Int, &String));
        assert!(!type_assignable(&Bool, &String));
        // Cross-kind mismatches.
        assert!(!type_assignable(&String, &Int));
        assert!(!type_assignable(&String, &Bool));
        assert!(!type_assignable(&Int, &Bool));
    }
}
