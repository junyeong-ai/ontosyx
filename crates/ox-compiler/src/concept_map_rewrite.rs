//! Concept-map literal translation for `QueryIR`.
//!
//! When a property's allowed values changed code-system between two
//! ontology snapshots (the canonical use case: a regulatory body
//! re-coded their standard — 2024 "A001" becomes 2026 "NEWA001"), a
//! query authored in the old vocabulary needs its literal codes
//! rewritten to the new vocabulary before the compiler emits
//! Cypher.
//!
//! A `ConceptMapDef` is the authored translation table for one such
//! migration; the `TranslationTable` here is a
//! `(variable, property) → (&ConceptMapDef, TranslationPolicy)`
//! lookup the caller assembles from the runtime context. The
//! rewriter walks every literal code value in the query that's
//! bound to a matching `(variable, property)` and substitutes the
//! translated code.
//!
//! Two knobs control aggressiveness:
//!
//! - **Direction** (`Forward` vs `Reverse`) — forward is
//!   "source → target", reverse the inverse.
//! - **Equivalence whitelist** — filters which equivalences are
//!   eligible for automatic substitution. Defaults to
//!   `Equivalent` only; the caller may opt into
//!   `NarrowerThanTarget` / `BroaderThanTarget` when losing
//!   precision is acceptable.
//!
//! The rewriter returns a `RewriteReport` alongside the new
//! `QueryIR`, listing both fired translations and
//! `UntranslatedLiteral`s the policy refused so the admin UI can
//! warn.

use std::collections::HashMap;

use ox_core::property_key::PropertyKey;
use ox_core::types::PropertyValue;
use ox_core::variable_name::VariableName;
use ox_ontology::concept_map::{ConceptMapDef, Equivalence};
use ox_query_ir::query::{
    ComparisonOp, Expr, GraphPattern, PropertyFilter, QueryIR, QueryOp,
};

/// Substitution direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranslationDirection {
    Forward,
    Reverse,
}

/// Per-(variable, property) translation policy.
#[derive(Debug, Clone)]
pub struct TranslationPolicy {
    pub direction: TranslationDirection,
    /// Which equivalences are acceptable substitutions. Mappings
    /// outside this set stay as-is (original literal preserved, one
    /// `UntranslatedLiteral` entry emitted).
    pub equivalence_whitelist: Vec<Equivalence>,
}

impl Default for TranslationPolicy {
    fn default() -> Self {
        Self {
            direction: TranslationDirection::Forward,
            equivalence_whitelist: vec![Equivalence::Equivalent],
        }
    }
}

/// Lookup table the caller hands to `rewrite_concept_map_values`.
pub type TranslationTable<'a> =
    HashMap<(VariableName, PropertyKey), (&'a ConceptMapDef, TranslationPolicy)>;

/// One translation that fired during a rewrite. Surfaces in the
/// report so callers can trace "why did this literal become that
/// one?" without re-walking the query themselves.
#[derive(Debug, Clone, PartialEq)]
pub struct TranslationEvent {
    pub variable: String,
    pub property: String,
    pub from_code: String,
    pub to_codes: Vec<String>,
    pub concept_map: String,
    pub equivalence: Equivalence,
}

/// One literal the rewriter could **not** translate. The caller
/// decides the policy: warn, drop, fail.
#[derive(Debug, Clone, PartialEq)]
pub struct UntranslatedLiteral {
    pub variable: String,
    pub property: String,
    pub value: String,
    pub concept_map: String,
}

/// Report returned alongside the rewritten query.
#[derive(Debug, Clone, Default)]
pub struct RewriteReport {
    pub translations: Vec<TranslationEvent>,
    pub untranslated: Vec<UntranslatedLiteral>,
}

/// Rewrite `query` according to `translations`. Returns the
/// rewritten query plus a `RewriteReport` listing what changed and
/// what the rewriter refused to touch.
///
/// The rewriter walks:
/// - inline `property_filters` on `GraphPattern::Node` and
///   `GraphPattern::Relationship` (the `{key: value}` form),
/// - WHERE expressions — both `Expr::Comparison` on a property
///   reference (`variable.property = 'value'`) and `Expr::In { expr:
///   Property, values }`.
pub fn rewrite_concept_map_values(
    mut query: QueryIR,
    translations: &TranslationTable<'_>,
) -> (QueryIR, RewriteReport) {
    let mut report = RewriteReport::default();
    if !translations.is_empty() {
        rewrite_op(&mut query.operation, translations, &mut report);
    }
    (query, report)
}

fn rewrite_op(
    op: &mut QueryOp,
    table: &TranslationTable<'_>,
    report: &mut RewriteReport,
) {
    match op {
        QueryOp::Match {
            patterns,
            filter,
            projections: _,
            optional: _,
            group_by: _,
        } => {
            for p in patterns {
                rewrite_pattern(p, table, report);
            }
            if let Some(f) = filter {
                rewrite_expr(f, table, report);
            }
        }
        QueryOp::Aggregate {
            source,
            group_by: _,
            aggregations: _,
            having,
        } => {
            rewrite_op(&mut source.operation, table, report);
            if let Some(f) = having {
                rewrite_expr(f, table, report);
            }
        }
        QueryOp::Union { queries, all: _ } => {
            for q in queries {
                rewrite_op(&mut q.operation, table, report);
            }
        }
        QueryOp::Chain { steps } => {
            for step in steps {
                rewrite_op(&mut step.operation, table, report);
            }
        }
        QueryOp::CallSubquery {
            inner,
            import_variables: _,
        } => {
            rewrite_op(&mut inner.operation, table, report);
        }
        // The first slice covers Match + Aggregate + composite
        // wrappers; Mutate / Analytics / PathFind carry literals in
        // shapes different enough that bespoke walkers land in
        // follow-up slices when a concrete use case arrives.
        QueryOp::Mutate { .. } | QueryOp::Analytics { .. } | QueryOp::PathFind { .. } => {}
    }
}

fn rewrite_pattern(
    pattern: &mut GraphPattern,
    table: &TranslationTable<'_>,
    report: &mut RewriteReport,
) {
    match pattern {
        GraphPattern::Node {
            variable,
            property_filters,
            ..
        } => {
            for pf in property_filters {
                rewrite_property_filter(variable, pf, table, report);
            }
        }
        GraphPattern::Relationship {
            variable,
            property_filters,
            ..
        } => {
            // Relationship patterns may carry an optional variable;
            // without a variable the inline filters can't be routed
            // (no `(variable, property)` key), so skip quietly.
            let Some(variable) = variable else { return };
            for pf in property_filters {
                rewrite_property_filter(variable, pf, table, report);
            }
        }
        GraphPattern::Path { elements: _ } => {}
    }
}

fn rewrite_property_filter(
    variable: &VariableName,
    pf: &mut PropertyFilter,
    table: &TranslationTable<'_>,
    report: &mut RewriteReport,
) {
    let key = (variable.clone(), pf.property.clone());
    let Some((concept_map, policy)) = table.get(&key) else {
        return;
    };
    rewrite_value_in_place(
        variable,
        &pf.property,
        &mut pf.value,
        concept_map,
        policy,
        report,
    );
}

fn rewrite_expr(
    expr: &mut Expr,
    table: &TranslationTable<'_>,
    report: &mut RewriteReport,
) {
    match expr {
        Expr::Comparison { left, op, right } => {
            rewrite_expr(left, table, report);
            rewrite_expr(right, table, report);
            if matches!(op, ComparisonOp::Eq | ComparisonOp::Neq) {
                rewrite_comparison(left.as_ref(), right.as_mut(), table, report);
                // Also handle operand-order reversed shape.
                rewrite_comparison(right.as_ref(), left.as_mut(), table, report);
            }
        }
        Expr::Logical { left, right, .. } => {
            rewrite_expr(left, table, report);
            rewrite_expr(right, table, report);
        }
        Expr::Not { inner } => rewrite_expr(inner, table, report),
        Expr::In { expr: inner, values } => {
            if let Some((variable, property)) = extract_property_ref(inner.as_ref()) {
                if let Some((concept_map, policy)) =
                    table.get(&(variable.clone(), property.clone()))
                {
                    rewrite_in_values(
                        &variable, &property, values, concept_map, policy, report,
                    );
                }
            }
        }
        _ => {}
    }
}

fn rewrite_comparison(
    anchor: &Expr,
    literal: &mut Expr,
    table: &TranslationTable<'_>,
    report: &mut RewriteReport,
) {
    let Some((variable, property)) = extract_property_ref(anchor) else {
        return;
    };
    let Some((concept_map, policy)) = table.get(&(variable.clone(), property.clone())) else {
        return;
    };
    rewrite_value_in_place(&variable, &property, literal, concept_map, policy, report);
}

fn extract_property_ref(expr: &Expr) -> Option<(VariableName, PropertyKey)> {
    match expr {
        Expr::Property {
            variable,
            field: Some(field),
        } => Some((variable.clone(), field.clone())),
        _ => None,
    }
}

fn rewrite_value_in_place(
    variable: &VariableName,
    property: &PropertyKey,
    value: &mut Expr,
    concept_map: &ConceptMapDef,
    policy: &TranslationPolicy,
    report: &mut RewriteReport,
) {
    let Expr::Literal {
        value: PropertyValue::String(original),
    } = value
    else {
        return;
    };
    let original = original.clone();
    let targets = translate(concept_map, &original, policy);
    if targets.is_empty() {
        report.untranslated.push(UntranslatedLiteral {
            variable: variable.to_string(),
            property: property.to_string(),
            value: original,
            concept_map: concept_map.name.clone(),
        });
        return;
    }
    // Substitute the first target in place; when more than one target
    // exists (one-to-many mapping), record all of them in the event so
    // the caller can rewrite the enclosing filter into an `In` shape
    // if it wants exact semantics. The in-place substitution still
    // preserves runnability: picking the first target is safer than
    // leaving the stale value in a `=` comparison.
    let first = &targets[0];
    *value = Expr::Literal {
        value: PropertyValue::String(first.target_code.clone()),
    };
    report.translations.push(TranslationEvent {
        variable: variable.to_string(),
        property: property.to_string(),
        from_code: original,
        to_codes: targets.iter().map(|t| t.target_code.clone()).collect(),
        concept_map: concept_map.name.clone(),
        equivalence: first.equivalence,
    });
}

fn rewrite_in_values(
    variable: &VariableName,
    property: &PropertyKey,
    values: &mut Vec<PropertyValue>,
    concept_map: &ConceptMapDef,
    policy: &TranslationPolicy,
    report: &mut RewriteReport,
) {
    let mut rewritten: Vec<PropertyValue> = Vec::with_capacity(values.len());
    for v in values.drain(..) {
        let PropertyValue::String(original) = v else {
            rewritten.push(v);
            continue;
        };
        let targets = translate(concept_map, &original, policy);
        if targets.is_empty() {
            report.untranslated.push(UntranslatedLiteral {
                variable: variable.to_string(),
                property: property.to_string(),
                value: original.clone(),
                concept_map: concept_map.name.clone(),
            });
            rewritten.push(PropertyValue::String(original));
            continue;
        }
        for t in &targets {
            rewritten.push(PropertyValue::String(t.target_code.clone()));
        }
        report.translations.push(TranslationEvent {
            variable: variable.to_string(),
            property: property.to_string(),
            from_code: original,
            to_codes: targets.iter().map(|t| t.target_code.clone()).collect(),
            concept_map: concept_map.name.clone(),
            equivalence: targets[0].equivalence,
        });
    }
    *values = rewritten;
}

fn translate(
    concept_map: &ConceptMapDef,
    code: &str,
    policy: &TranslationPolicy,
) -> Vec<ox_ontology::concept_map::Translation> {
    let raw = match policy.direction {
        TranslationDirection::Forward => concept_map.translate(code),
        TranslationDirection::Reverse => concept_map.translate_reverse(code),
    };
    raw.into_iter()
        .filter(|t| policy.equivalence_whitelist.contains(&t.equivalence))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::graph_label::GraphLabel;
    use ox_ontology::code_system::CodeSystemId;
    use ox_ontology::concept_map::{ConceptMapId, ConceptMapping, Equivalence};
    use ox_query_ir::query::{GraphPattern, Projection, QueryIR, QueryOp};

    fn vn(s: &str) -> VariableName {
        VariableName::new(s).expect("valid var")
    }

    fn pk(s: &str) -> PropertyKey {
        PropertyKey::new(s).expect("valid property key")
    }

    fn concept_map() -> ConceptMapDef {
        ConceptMapDef {
            id: ConceptMapId::new("cm-v2024-to-v2026"),
            name: "krx_migration".into(),
            display_name: Default::default(),
            description: Default::default(),
            version: "1".into(),
            source_system_id: CodeSystemId::new("cs-2024"),
            target_system_id: CodeSystemId::new("cs-2026"),
            mappings: vec![
                ConceptMapping {
                    source_code: "A001".into(),
                    target_code: "NEWA001".into(),
                    equivalence: Equivalence::Equivalent,
                    comment: Default::default(),
                },
                ConceptMapping {
                    source_code: "B002".into(),
                    target_code: "NEWB002".into(),
                    equivalence: Equivalence::BroaderThanTarget,
                    comment: Default::default(),
                },
                ConceptMapping {
                    source_code: "C003".into(),
                    target_code: "NEWC003A".into(),
                    equivalence: Equivalence::Equivalent,
                    comment: Default::default(),
                },
                ConceptMapping {
                    source_code: "C003".into(),
                    target_code: "NEWC003B".into(),
                    equivalence: Equivalence::Equivalent,
                    comment: Default::default(),
                },
            ],
        }
    }

    fn build_match_query(variable: &'static str, code: &str) -> QueryIR {
        QueryIR {
            schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn(variable),
                    label: Some(GraphLabel::new("Stock").unwrap()),
                    property_filters: vec![PropertyFilter {
                        property: pk("status"),
                        value: Expr::Literal {
                            value: PropertyValue::String(code.into()),
                        },
                    }],
                }],
                filter: None,
                projections: vec![Projection::Variable { variable: vn(variable), alias: None }],
                optional: false,
                group_by: Vec::new(),
            },
            limit: None,
            skip: None,
            order_by: Vec::new(),
            as_of: None,
        }
    }

    fn table<'a>(cm: &'a ConceptMapDef, variable: &str) -> TranslationTable<'a> {
        let mut t = TranslationTable::new();
        t.insert(
            (vn(variable), pk("status")),
            (cm, TranslationPolicy::default()),
        );
        t
    }

    fn read_pattern_literal(ir: &QueryIR) -> String {
        let QueryOp::Match { patterns, .. } = &ir.operation else {
            panic!("expected Match");
        };
        let GraphPattern::Node {
            property_filters, ..
        } = &patterns[0]
        else {
            panic!("expected Node");
        };
        let Expr::Literal {
            value: PropertyValue::String(s),
        } = &property_filters[0].value
        else {
            panic!("expected string literal");
        };
        s.clone()
    }

    #[test]
    fn equivalent_mapping_is_substituted_forward() {
        let cm = concept_map();
        let (rewritten, report) =
            rewrite_concept_map_values(build_match_query("s", "A001"), &table(&cm, "s"));
        assert_eq!(report.translations.len(), 1);
        assert!(report.untranslated.is_empty());
        assert_eq!(read_pattern_literal(&rewritten), "NEWA001");
    }

    #[test]
    fn broader_than_target_is_skipped_by_default_policy() {
        let cm = concept_map();
        let (rewritten, report) =
            rewrite_concept_map_values(build_match_query("s", "B002"), &table(&cm, "s"));
        assert!(report.translations.is_empty());
        assert_eq!(report.untranslated.len(), 1);
        assert_eq!(
            read_pattern_literal(&rewritten),
            "B002",
            "original literal must be preserved on refusal"
        );
    }

    #[test]
    fn broader_than_target_is_applied_when_whitelisted() {
        let cm = concept_map();
        let mut t = TranslationTable::new();
        t.insert(
            (vn("s"), pk("status")),
            (
                &cm,
                TranslationPolicy {
                    direction: TranslationDirection::Forward,
                    equivalence_whitelist: vec![
                        Equivalence::Equivalent,
                        Equivalence::BroaderThanTarget,
                    ],
                },
            ),
        );
        let (_, report) = rewrite_concept_map_values(build_match_query("s", "B002"), &t);
        assert_eq!(report.translations.len(), 1);
    }

    #[test]
    fn one_to_many_mapping_records_every_target_in_event() {
        let cm = concept_map();
        let (_, report) =
            rewrite_concept_map_values(build_match_query("s", "C003"), &table(&cm, "s"));
        assert_eq!(report.translations.len(), 1);
        assert_eq!(
            report.translations[0].to_codes,
            vec!["NEWC003A", "NEWC003B"]
        );
    }

    #[test]
    fn unmapped_code_surfaces_as_untranslated() {
        let cm = concept_map();
        let (_, report) =
            rewrite_concept_map_values(build_match_query("s", "ZZZ"), &table(&cm, "s"));
        assert!(report.translations.is_empty());
        assert_eq!(report.untranslated.len(), 1);
    }

    #[test]
    fn no_match_variable_is_ignored() {
        let cm = concept_map();
        // Table keyed on `other`, query uses `s`.
        let mut t = TranslationTable::new();
        t.insert(
            (vn("other"), pk("status")),
            (&cm, TranslationPolicy::default()),
        );
        let (_, report) = rewrite_concept_map_values(build_match_query("s", "A001"), &t);
        assert!(report.translations.is_empty());
        assert!(report.untranslated.is_empty());
    }

    #[test]
    fn reverse_direction_maps_target_back_to_source() {
        let cm = concept_map();
        let mut t = TranslationTable::new();
        t.insert(
            (vn("s"), pk("status")),
            (
                &cm,
                TranslationPolicy {
                    direction: TranslationDirection::Reverse,
                    equivalence_whitelist: vec![Equivalence::Equivalent],
                },
            ),
        );
        let (_, report) = rewrite_concept_map_values(build_match_query("s", "NEWA001"), &t);
        assert_eq!(report.translations.len(), 1);
        assert_eq!(report.translations[0].to_codes, vec!["A001"]);
    }

    #[test]
    fn where_comparison_literal_is_translated() {
        let cm = concept_map();
        let q = QueryIR {
            schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("s"),
                    label: Some(GraphLabel::new("Stock").unwrap()),
                    property_filters: Vec::new(),
                }],
                filter: Some(Expr::Comparison {
                    left: Box::new(Expr::Property {
                        variable: vn("s"),
                        field: Some(pk("status")),
                    }),
                    op: ComparisonOp::Eq,
                    right: Box::new(Expr::Literal {
                        value: PropertyValue::String("A001".into()),
                    }),
                }),
                projections: vec![Projection::Variable { variable: vn("s"), alias: None }],
                optional: false,
                group_by: Vec::new(),
            },
            limit: None,
            skip: None,
            order_by: Vec::new(),
            as_of: None,
        };
        let (rewritten, report) = rewrite_concept_map_values(q, &table(&cm, "s"));
        assert_eq!(report.translations.len(), 1);
        let QueryOp::Match { filter, .. } = rewritten.operation else {
            panic!("expected Match");
        };
        let Some(Expr::Comparison { right, .. }) = filter else {
            panic!("expected Comparison");
        };
        let Expr::Literal {
            value: PropertyValue::String(s),
        } = *right
        else {
            panic!("expected string literal");
        };
        assert_eq!(s, "NEWA001");
    }

    #[test]
    fn where_in_clause_expands_literals() {
        let cm = concept_map();
        let q = QueryIR {
            schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("s"),
                    label: Some(GraphLabel::new("Stock").unwrap()),
                    property_filters: Vec::new(),
                }],
                filter: Some(Expr::In {
                    expr: Box::new(Expr::Property {
                        variable: vn("s"),
                        field: Some(pk("status")),
                    }),
                    values: vec![
                        PropertyValue::String("A001".into()),
                        PropertyValue::String("C003".into()),
                    ],
                }),
                projections: vec![Projection::Variable { variable: vn("s"), alias: None }],
                optional: false,
                group_by: Vec::new(),
            },
            limit: None,
            skip: None,
            order_by: Vec::new(),
            as_of: None,
        };
        let (rewritten, report) = rewrite_concept_map_values(q, &table(&cm, "s"));
        assert_eq!(report.translations.len(), 2);
        let QueryOp::Match { filter, .. } = rewritten.operation else {
            panic!("expected Match");
        };
        let Some(Expr::In { values, .. }) = filter else {
            panic!("expected In");
        };
        // "A001" → "NEWA001" (1 target), "C003" → "NEWC003A" + "NEWC003B" (2 targets).
        let strings: Vec<String> = values
            .into_iter()
            .filter_map(|v| match v {
                PropertyValue::String(s) => Some(s),
                _ => None,
            })
            .collect();
        assert_eq!(strings, vec!["NEWA001", "NEWC003A", "NEWC003B"]);
    }
}
