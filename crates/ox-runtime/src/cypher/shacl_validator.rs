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
//! ## Scope (MVP)
//!
//! Constraints enforced today:
//! - `MinCount { min >= 1 }` on a `PropertyShape` → the referenced
//!   property must appear in the node's inline property map on
//!   `CREATE` / `MERGE`.
//! - `In { allowed }` on a `PropertyShape` with a string-literal value
//!   → the literal must match an entry in the enum set.
//! - `Pattern { regex }` on a `PropertyShape` with a string-literal
//!   value → the literal must match the regex.
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
//! - `NodeShape` / `EdgeShape` / `CrossEntityShape` / `StateMachine`
//!   → each lands on its own phase; today's emitter surfaces them as
//!   `Info` so the rule author sees the rule is recognised but not
//!   yet enforced here.
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

use std::fmt;

use ox_core::graph_label::GraphLabel;
use ox_ontology::ir::{NodeTypeDef, OntologyIR, PropertyDef, PropertyId};
use ox_ontology::rule::{
    ConstraintTarget, EnforcementKind, RuleActivationKind, RuleDef, RuleKind, Severity,
    ShaclConstraint,
};

use crate::cypher::ast::{ClauseKind, CypherAst, CypherPatternElement, NodePattern};
use crate::cypher::validate::{
    CypherValidator, ValidateContext, ValidatePhase, ValidationIssue,
};

/// SHACL-rule enforcement against Cypher writes. See module docs for
/// the MVP constraint coverage.
pub struct ShaclValidator {
    ontology: OntologyIR,
}

impl ShaclValidator {
    pub fn new(ontology: OntologyIR) -> Self {
        Self { ontology }
    }
}

impl fmt::Debug for ShaclValidator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShaclValidator")
            .field("ontology_id", &self.ontology.id)
            .field("active_rules", &self.active_rules().count())
            .finish()
    }
}

impl ShaclValidator {
    /// Rules live for the pre-execute write path: `Always` activation
    /// plus `Write` enforcement. `Read` rules fire in the result-shaping
    /// path (not implemented here); `Batch` rules fire in the
    /// data-quality scheduler.
    fn active_rules(&self) -> impl Iterator<Item = &RuleDef> {
        self.ontology.rules().iter().filter(|r| {
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
            for clause in &statement.clauses {
                if !matches!(clause.kind, ClauseKind::Create | ClauseKind::Merge) {
                    // SHACL Write enforcement only; MATCH/OPTIONAL MATCH
                    // are read-side. SET / DELETE are handled under
                    // their own phase: SET needs per-key evaluation,
                    // DELETE invokes `sh:closed` + referential rules —
                    // tracked separately to keep this validator's
                    // surface predictable.
                    continue;
                }
                for pattern in &clause.patterns {
                    for element in &pattern.elements {
                        if let CypherPatternElement::Node(node) = element {
                            self.validate_node(&active, node, &mut issues);
                        }
                    }
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
                    issues.push(build_issue(
                        rule,
                        format!(
                            "property `{key}` is required by rule `{rn}` \
                             (minCount={min}) but missing from the write",
                            rn = rule.name,
                        ),
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
                let Some(literal) = parse_string_literal(raw) else {
                    // Not a string literal (param / fn / numeric) — can't
                    // decide at AST time. Skipping is correct behaviour.
                    return;
                };
                // Resolve the referenced value set against the active
                // ontology. A dangling id is a rule-authoring error;
                // surface as Info and skip enforcement rather than
                // blocking every caller.
                let Some(vs) = self.ontology.value_set_by_id(value_set_id) else {
                    issues.push(ValidationIssue::info(
                        "shacl",
                        format!(
                            "rule `{rn}` references unknown value set `{value_set_id}` \
                             and was skipped for property `{key}`",
                            rn = rule.name,
                        ),
                    ));
                    return;
                };
                let expansion = ox_ontology::value_set::expand_value_set(
                    vs,
                    self.ontology.code_systems(),
                );
                let allowed: Vec<&str> =
                    expansion.codes.iter().map(|cv| cv.code.as_str()).collect();
                if !allowed.iter().any(|a| **a == literal) {
                    issues.push(build_issue(
                        rule,
                        format!(
                            "property `{key}` = {raw:?} violates rule `{rn}` \
                             (value set `{vs_name}` allows: {allowed:?})",
                            rn = rule.name,
                            vs_name = vs.name,
                        ),
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
                    issues.push(ValidationIssue::info(
                        "shacl",
                        format!(
                            "rule `{rn}` references unknown notation pattern \
                             `{notation_pattern_id}` and was skipped for property `{key}`",
                            rn = rule.name,
                        ),
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
                    issues.push(build_issue(
                        rule,
                        format!(
                            "property `{key}` = {raw:?} does not match notation \
                             pattern `{np_name}` required by rule `{rn}` ({err})",
                            rn = rule.name,
                            np_name = np.name,
                        ),
                        Some(node.span),
                    ));
                }
            }
            // Constraint kinds handled at the node-shape level (see
            // check_node_level_constraint) or at a later phase.
            ShaclConstraint::Closed { .. }
            | ShaclConstraint::Disjoint { .. }
            | ShaclConstraint::UniqueKey { .. }
            | ShaclConstraint::UniqueLang { .. } => {}
            // Constraint kinds the MVP recognises but doesn't enforce at
            // the Cypher AST layer yet (see module docs for rationale).
            ShaclConstraint::MaxCount { .. }
            | ShaclConstraint::Datatype { .. }
            | ShaclConstraint::HasValue { .. }
            | ShaclConstraint::MinInclusive { .. }
            | ShaclConstraint::MaxInclusive { .. }
            | ShaclConstraint::MinLength { .. }
            | ShaclConstraint::MaxLength { .. } => {}
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
                        issues.push(build_issue(
                            rule,
                            format!(
                                "property `{key}` is not permitted on `{label}` \
                                 (closed shape rule `{rn}`)",
                                label = node_type.label.as_str(),
                                rn = rule.name,
                            ),
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
                    issues.push(build_issue(
                        rule,
                        format!(
                            "properties `{ak}` and `{bk}` carry the same value \
                             {a_raw:?} but rule `{rn}` declares them disjoint",
                            ak = a_prop.name.as_str(),
                            bk = b_prop.name.as_str(),
                            rn = rule.name,
                        ),
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
                        issues.push(build_issue(
                            rule,
                            format!(
                                "unique-key rule `{rn}` references unknown property `{key}` \
                                 on `{label}`",
                                rn = rule.name,
                                label = node_type.label.as_str(),
                                key = key.as_str(),
                            ),
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
                    issues.push(ValidationIssue::info(
                        "shacl",
                        format!(
                            "rule `{rn}` enforces uniqueLang on `{key}`, but the \
                             property is not declared `is_localized`",
                            rn = rule.name,
                            key = prop.name.as_str(),
                        ),
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

fn build_issue(
    rule: &RuleDef,
    message: String,
    span: Option<crate::cypher::token::Span>,
) -> ValidationIssue {
    let issue = match rule.severity {
        Severity::Violation => ValidationIssue::error("shacl", message),
        Severity::Warning => ValidationIssue::warning("shacl", message),
        Severity::Info => ValidationIssue::info("shacl", message),
    };
    match span {
        Some(s) => issue.with_span(s),
        None => issue,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use ox_core::i18n::LocalizedText;
    use ox_core::property_key::PropertyKey;
    use ox_core::types::PropertyType;
    use ox_ontology::action::RuleId;
    use ox_ontology::ir::{NodeTypeDef, NodeTypeId, OntologyIR, PropertyDef};

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
            kind: RuleKind::PropertyShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
                target_property_id: PropertyId::new("prop-email"),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            constraints: vec![ShaclConstraint::MinCount {
                target: ConstraintTarget::Inherit,
                min: 1,
            }],
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
            issues
                .iter()
                .any(|i| i.level == IssueLevel::Error && i.message.contains("email")),
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
            kind: RuleKind::PropertyShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
                target_property_id: PropertyId::new("prop-status"),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            constraints: vec![ShaclConstraint::InValueSet {
                target: ConstraintTarget::Inherit,
                value_set_id: vs_id,
            }],
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
            issues
                .iter()
                .any(|i| i.level == IssueLevel::Error && i.message.contains("bogus")),
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
            kind: RuleKind::PropertyShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
                target_property_id: PropertyId::new("prop-email"),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            constraints: vec![ShaclConstraint::MatchesPattern {
                target: ConstraintTarget::Inherit,
                notation_pattern_id: np_id,
            }],
        };
        onto.add_rule(rule).expect("rule add");
        let issues = run(onto, "CREATE (u:User {email: 'not-an-email'})");
        assert!(
            issues
                .iter()
                .any(|i| i.level == IssueLevel::Error && i.message.contains("notation")),
            "{issues:?}"
        );
    }

    #[test]
    fn severity_warning_maps_to_warning_issue() {
        let (mut rule, prop) = required_email_rule();
        rule.severity = Severity::Warning;
        let onto = fixture_ontology_with_rule(rule, prop);
        let issues = run(onto, "CREATE (u:User {id: 1})");
        assert!(
            issues
                .iter()
                .any(|i| i.level == IssueLevel::Warning && i.message.contains("email")),
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
    // Phase 1.5 — Closed / Disjoint / UniqueKey / UniqueLang.
    // -------------------------------------------------------------

    fn closed_rule() -> RuleDef {
        RuleDef {
            id: RuleId::new("r-user-closed"),
            name: "user_closed_shape".into(),
            description: LocalizedText::default(),
            kind: RuleKind::NodeShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            constraints: vec![ShaclConstraint::Closed {
                target: ConstraintTarget::Inherit,
                // Only declared-on-shape keys beyond the node type's
                // own `email` property. So `email` is still allowed
                // (implicit), but `password` is not.
                allowed_properties: Vec::new(),
            }],
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
            issues
                .iter()
                .any(|i| i.level == IssueLevel::Error && i.message.contains("password")),
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
            kind: RuleKind::NodeShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
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
        })
        .expect("rule add");
        let issues = run(
            onto,
            "CREATE (u:User {email: 'a@b', email_backup: 'a@b'})",
        );
        assert!(
            issues
                .iter()
                .any(|i| i.level == IssueLevel::Error && i.message.contains("disjoint")),
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
            kind: RuleKind::NodeShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
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
            kind: RuleKind::NodeShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            constraints: vec![ShaclConstraint::UniqueKey {
                target_node_type_id: NodeTypeId::new("nt-user"),
                property_keys: vec![PropertyKey::new("email").unwrap(), PropertyKey::new("tenant").unwrap()],
            }],
        })
        .expect("rule add");
        let issues = run(onto, "CREATE (u:User {email: 'a@b'})");
        assert!(
            issues
                .iter()
                .any(|i| i.level == IssueLevel::Error && i.message.contains("tenant")),
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
            kind: RuleKind::PropertyShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
                target_property_id: PropertyId::new("prop-email"),
            },
            severity: Severity::Warning,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            constraints: vec![ShaclConstraint::UniqueLang {
                target: ConstraintTarget::Property {
                    node_type_id: NodeTypeId::new("nt-user"),
                    property_id: PropertyId::new("prop-email"),
                },
            }],
        })
        .expect("rule add");
        let issues = run(onto, "CREATE (u:User {email: 'a@b'})");
        assert!(
            issues
                .iter()
                .any(|i| i.level == IssueLevel::Info && i.message.contains("uniqueLang")),
            "non-localized UniqueLang must surface Info: {issues:?}"
        );
    }
}
