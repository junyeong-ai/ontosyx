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
    AnalyticsSource, ComparisonOp, Expr, GraphPattern, MutateOp, PropertyAssignment,
    PropertyFilter, QueryIR, QueryOp,
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
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UntranslatedLiteral {
    pub variable: String,
    pub property: String,
    pub value: String,
    pub concept_map: String,
}

/// Report returned alongside the rewritten query.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RewriteReport {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub translations: Vec<TranslationEvent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub untranslated: Vec<UntranslatedLiteral>,
}

impl RewriteReport {
    /// `true` when no translations fired and no untranslated literal
    /// was reported. The compiler short-circuits the rewrite when the
    /// translation table is empty, so the report stays empty for the
    /// common no-concept-map case and serialises to `{}` (or omits
    /// entirely on the `CompiledQuery` wire shape via
    /// `skip_serializing_if`).
    pub fn is_empty(&self) -> bool {
        self.translations.is_empty() && self.untranslated.is_empty()
    }
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

/// Build a `TranslationTable` for the given query against the
/// ontology's concept-map registry. Walks the query's match patterns
/// to discover `(variable, property)` pairs whose property is bound
/// to a value-set, then for every concept-map whose source code
/// system overlaps the value-set's includes, registers a default
/// (forward, equivalent-only) translation.
///
/// Out of scope:
/// - Edge-bound properties (today only Node patterns get scanned).
///   Edge property translation lands when an edge property carries a
///   value-set in production — currently a hypothetical case the IR
///   permits but no shipped feature uses.
/// - Custom translation policies. Every entry uses
///   `TranslationPolicy::default()` (Forward + Equivalent only). A
///   per-property policy table can be threaded through later when an
///   admin UI surfaces the equivalence-whitelist toggle.
///
/// Returns an empty table when the ontology has no concept-maps; the
/// rewriter then short-circuits and the caller pays no traversal
/// cost.
pub fn build_translation_table_for_query<'a>(
    query: &QueryIR,
    ontology: &'a ox_ontology::OntologyIR,
) -> TranslationTable<'a> {
    let mut table: TranslationTable<'a> = HashMap::new();
    if ontology.concept_maps().is_empty() {
        return table;
    }
    walk_match_patterns(&query.operation, &mut |variable, label| {
        let Some(node) = ontology.node_by_label(label.as_str()) else {
            return;
        };
        for prop in &node.properties {
            // Resolution order:
            // 1. Explicit binding-level concept_map_id (semantic
            //    normalisation declared by the property author).
            // 2. Explicit mapping-level concept_map_id (physical
            //    normalisation declared by the mapping author).
            // 3. Inference from the property's value-set composition
            //    (fallback when neither author was explicit).
            let explicit = prop
                .value_set_binding()
                .and_then(|b| b.concept_map_id())
                .or_else(|| {
                    ontology
                        .object_mappings()
                        .iter()
                        .filter(|om| om.node_type_id == node.id)
                        .flat_map(|om| om.property_mappings.iter())
                        .find(|pm| pm.property_id == prop.id)
                        .and_then(|pm| pm.concept_map_id.as_ref())
                });

            if let Some(cm_id) = explicit {
                if let Some(cm) =
                    ontology.concept_maps().iter().find(|cm| cm.id == *cm_id)
                {
                    table.insert(
                        (variable.clone(), prop.name.clone()),
                        (cm, TranslationPolicy::default()),
                    );
                }
                continue;
            }

            // Inferred path — only triggers when no explicit
            // concept_map_id was declared on either surface.
            let Some(value_set_id) = prop.value_set_id() else {
                continue;
            };
            let Some(value_set) = ontology.value_set_by_id(value_set_id) else {
                continue;
            };
            let referenced_systems: std::collections::HashSet<_> = value_set
                .composition
                .iter()
                .map(|inc| inc.system_id.clone())
                .collect();
            for cm in ontology.concept_maps() {
                if referenced_systems.contains(&cm.source_system_id) {
                    table.insert(
                        (variable.clone(), prop.name.clone()),
                        (cm, TranslationPolicy::default()),
                    );
                }
            }
        }
    });
    table
}

/// Visitor over every `GraphPattern::Node` reachable through the
/// query's QueryOp tree. Calls `f(variable, label)` for each node
/// pattern that has a label binding — patterns without a label
/// (anonymous walks) are skipped because they don't pin a node type.
fn walk_match_patterns<F>(op: &QueryOp, f: &mut F)
where
    F: FnMut(&VariableName, &ox_core::graph_label::GraphLabel),
{
    match op {
        QueryOp::Match { patterns, .. } => {
            for pattern in patterns {
                if let GraphPattern::Node {
                    variable,
                    label: Some(label),
                    ..
                } = pattern
                {
                    f(variable, label);
                }
            }
        }
        QueryOp::Aggregate { source, .. } => walk_match_patterns(&source.operation, f),
        QueryOp::Union { queries, .. } => {
            for q in queries {
                walk_match_patterns(&q.operation, f);
            }
        }
        QueryOp::Chain { steps } => {
            for s in steps {
                walk_match_patterns(&s.operation, f);
            }
        }
        QueryOp::CallSubquery { inner, .. } => walk_match_patterns(&inner.operation, f),
        QueryOp::Mutate {
            context, operations, ..
        } => {
            if let Some(ctx) = context {
                walk_match_patterns(ctx, f);
            }
            // Mutate ops with a *node* label (CreateNode / MergeNode)
            // pin a (variable, label) pair the caller's translation
            // table needs to dispatch property-level translations on
            // the new row's property assignments. CreateEdge /
            // MergeEdge carry an edge label, but the translation
            // table is keyed on node-property pairs (the only surface
            // a ConceptMap currently binds against), so edges
            // contribute nothing here.
            for op in operations {
                match op {
                    MutateOp::CreateNode {
                        variable, label, ..
                    }
                    | MutateOp::MergeNode {
                        variable, label, ..
                    } => f(variable, label),
                    _ => {}
                }
            }
        }
        QueryOp::Analytics { source, .. } => {
            // Whole-graph / labels-only analytics carries no
            // pattern variables to anchor on. A subgraph-source
            // recurses into the embedded filter so its patterns
            // contribute labels just like a top-level Match.
            if let AnalyticsSource::Subgraph { filter } = source {
                walk_match_patterns(filter, f);
            }
        }
        QueryOp::PathFind { start, end, .. } => {
            if let Some(label) = &start.label {
                f(&start.variable, label);
            }
            if let Some(label) = &end.label {
                f(&end.variable, label);
            }
        }
        QueryOp::HybridSearch { .. } => {
            // Hybrid retrieval result set isn't pattern-shaped —
            // there's no `(variable, label)` binding for the
            // ConceptMap rewriter to anchor property-level
            // translations on. The optional `graph_constraints`
            // pattern doesn't bind variables to projections, so
            // it doesn't contribute either.
        }
    }
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
        QueryOp::Mutate {
            context,
            operations,
            returning: _,
        } => {
            // Recurse into the optional preceding MATCH so any
            // anchor-bearing patterns are still subject to literal
            // translation. The mutating operations themselves are
            // walked individually — their property-assignment shape
            // is distinct from `Match.filter`'s `Expr` tree.
            if let Some(ctx) = context.as_mut() {
                rewrite_op(ctx, table, report);
            }
            for op in operations {
                rewrite_mutate_op(op, table, report);
            }
        }
        QueryOp::Analytics {
            source,
            params: _,
            algorithm: _,
            projections: _,
        } => {
            // ConceptMap dispatch is keyed on (variable, property)
            // pairs. `Analytics.params` is a `HashMap<String, Expr>`
            // with only string keys (algorithm config — damping
            // factor, iteration count) so there is no anchor to
            // translate against. A subgraph source is the only
            // anchor-bearing surface inside Analytics; recurse so
            // its Match patterns get the same treatment as a
            // top-level Match.
            if let AnalyticsSource::Subgraph { filter } = source {
                rewrite_op(filter, table, report);
            }
        }
        QueryOp::PathFind {
            start,
            end,
            edge_types: _,
            direction: _,
            max_depth: _,
            algorithm: _,
        } => {
            // Endpoint inline filters mirror the `GraphPattern::Node`
            // shape, so the existing rewriter applies unchanged.
            for pf in &mut start.property_filters {
                rewrite_property_filter(&start.variable, pf, table, report);
            }
            for pf in &mut end.property_filters {
                rewrite_property_filter(&end.variable, pf, table, report);
            }
        }
        QueryOp::HybridSearch { .. } => {
            // Hybrid retrieval has no property-assignment or
            // property-filter surface for the rewriter to anchor
            // on. The vector / fulltext queries are opaque text;
            // the optional graph constraint sub-pattern doesn't
            // expose property filters that the existing
            // rewriter walks.
        }
    }
}

/// Substitute literal values inside a list of property assignments
/// (`Mutate` operations) when the `(variable, property)` pair binds
/// to a concept map in the translation table. Mirrors
/// `rewrite_property_filter` but operates on the assignment shape.
fn rewrite_property_assignments(
    variable: &VariableName,
    assignments: &mut [PropertyAssignment],
    table: &TranslationTable<'_>,
    report: &mut RewriteReport,
) {
    for pa in assignments {
        if let Some((concept_map, policy)) =
            table.get(&(variable.clone(), pa.property.clone()))
        {
            rewrite_value_in_place(
                variable,
                &pa.property,
                &mut pa.value,
                concept_map,
                policy,
                report,
            );
        }
    }
}

fn rewrite_mutate_op(
    op: &mut MutateOp,
    table: &TranslationTable<'_>,
    report: &mut RewriteReport,
) {
    match op {
        MutateOp::CreateNode {
            variable,
            label: _,
            properties,
        } => {
            rewrite_property_assignments(variable, properties, table, report);
        }
        MutateOp::CreateEdge {
            variable,
            label: _,
            source: _,
            target: _,
            properties,
        } => {
            // Edge variables are optional; absent variable = anonymous
            // edge whose properties cannot be anchored to a
            // (variable, property) translation key.
            if let Some(var) = variable {
                rewrite_property_assignments(var, properties, table, report);
            }
        }
        MutateOp::MergeNode {
            variable,
            label: _,
            match_properties,
            on_create,
            on_match,
        } => {
            rewrite_property_assignments(variable, match_properties, table, report);
            rewrite_property_assignments(variable, on_create, table, report);
            rewrite_property_assignments(variable, on_match, table, report);
        }
        MutateOp::MergeEdge {
            variable,
            label: _,
            source: _,
            target: _,
            match_properties,
            on_create,
            on_match,
        } => {
            if let Some(var) = variable {
                rewrite_property_assignments(var, match_properties, table, report);
                rewrite_property_assignments(var, on_create, table, report);
                rewrite_property_assignments(var, on_match, table, report);
            }
        }
        MutateOp::SetProperty {
            variable,
            property,
            value,
        } => {
            if let Some((concept_map, policy)) =
                table.get(&(variable.clone(), property.clone()))
            {
                rewrite_value_in_place(
                    variable,
                    property,
                    value,
                    concept_map,
                    policy,
                    report,
                );
            }
        }
        MutateOp::Delete { .. }
        | MutateOp::RemoveProperty { .. }
        | MutateOp::RemoveLabel { .. } => {
            // No literal substitution surface — these carry only
            // variable / property / label references.
        }
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
            if let Some((variable, property)) = extract_property_ref(inner.as_ref())
                && let Some((concept_map, policy)) =
                    table.get(&(variable.clone(), property.clone()))
            {
                rewrite_in_values(
                    &variable, &property, values, concept_map, policy, report,
                );
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
    fn build_translation_table_returns_empty_when_ontology_has_no_concept_maps() {
        use ox_core::i18n::LocalizedText;
        use ox_ontology::OntologyIR;
        let ir = OntologyIR::new(
            "ont".into(),
            "Test".into(),
            LocalizedText::default(),
            1,
            vec![],
            vec![],
            vec![],
        );
        let q = build_match_query("s", "A001");
        let table = build_translation_table_for_query(&q, &ir);
        assert!(table.is_empty(), "no concept maps → empty table");
    }

    #[test]
    fn build_translation_table_picks_concept_map_via_value_set_overlap() {
        // Wire an ontology that bolts together: a CodeSystem, a
        // ValueSet referencing it, a NodeType with a property bound
        // to the ValueSet, and a ConceptMap whose source matches the
        // CodeSystem. The helper must discover the (variable,
        // property) → ConceptMap mapping for a query against the
        // node's label.
        use ox_core::i18n::LocalizedText;
        use ox_core::types::PropertyType;
        use ox_ontology::OntologyIR;
        use ox_ontology::code_system::{
            CodeSystemDef, CodeSystemId as CsId, CodeSystemKind, CodedValue, CodedValueId,
        };
        use ox_ontology::ir::{NodeTypeDef, PropertyDef};
        use ox_ontology::value_set::{
            IncludeMode, ValueSetDef, ValueSetId, ValueSetIncludeRule, ValueSetSelector,
        };

        let cs_id = CsId::new("cs-2024");
        let cs = CodeSystemDef {
            id: cs_id.clone(),
            name: "krx-2024".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            kind: CodeSystemKind::Internal,
            uri: None,
            hierarchical: false,
            deprecated_at: None,
            replaced_by_id: None,
            codes: vec![CodedValue {
                id: CodedValueId::new("cv-A001"),
                code: "A001".into(),
                display: LocalizedText::default(),
                definition: LocalizedText::default(),
                aliases: Vec::new(),
                broader_id: None,
                examples: Vec::new(),
                scope_note: LocalizedText::default(),
                valid_from: None,
                valid_to: None,
                deprecated_at: None,
                replaced_by_id: None,
            }],
        };
        let vs = ValueSetDef {
            id: ValueSetId::new("vs-stock-status"),
            name: "stock_status".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            composition: vec![ValueSetIncludeRule {
                mode: IncludeMode::Include,
                system_id: cs_id.clone(),
                selector: ValueSetSelector::All,
            }],
        };
        let cm = concept_map();

        let mut ir = OntologyIR::new(
            "ont".into(),
            "Test".into(),
            LocalizedText::default(),
            1,
            vec![NodeTypeDef {
                id: "n-stock".into(),
                label: GraphLabel::new("Stock").expect("label"),
                description: LocalizedText::default(),
                properties: vec![PropertyDef {
                    id: "p-status".into(),
                    name: pk("status"),
                    property_type: PropertyType::String,
                    nullable: false,
                    bindings: vec![ox_ontology::PropertyBinding::value_set(ValueSetId::new("vs-stock-status"),)],
                    ..Default::default()
                }],
                constraints: vec![],
                ..Default::default()
            }],
            vec![],
            vec![],
        );
        // The ConceptMap referenced by `concept_map()` declares
        // cs-2024 → cs-2026, so the target CodeSystem must exist for
        // `add_concept_map` to accept it (referential integrity).
        let cs_target = CodeSystemDef {
            id: CsId::new("cs-2026"),
            name: "krx-2026".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            kind: CodeSystemKind::Internal,
            uri: None,
            hierarchical: false,
            deprecated_at: None,
            replaced_by_id: None,
            codes: Vec::new(),
        };
        ir.add_code_system(cs).expect("seed cs source");
        ir.add_code_system(cs_target).expect("seed cs target");
        ir.add_value_set(vs).expect("seed vs");
        ir.add_concept_map(cm).expect("seed cm");

        let q = build_match_query("s", "A001");
        let table = build_translation_table_for_query(&q, &ir);
        assert_eq!(table.len(), 1, "(s, status) → ConceptMap");
        assert!(table.contains_key(&(vn("s"), pk("status"))));

        // End-to-end: translation actually fires when the rewrite
        // runs against the discovered table.
        let (_, report) = rewrite_concept_map_values(q, &table);
        assert_eq!(report.translations.len(), 1);
        assert_eq!(report.translations[0].to_codes, vec!["NEWA001"]);
    }

    /// When a `PropertyBinding` declares an explicit `concept_map_id`,
    /// the explicit id wins over the value-set inference path even
    /// when the inferred map matches the same source system.
    #[test]
    fn build_translation_table_honours_explicit_binding_concept_map_id() {
        use ox_core::i18n::LocalizedText;
        use ox_core::types::PropertyType;
        use ox_ontology::OntologyIR;
        use ox_ontology::code_system::{
            CodeSystemDef, CodeSystemId as CsId, CodeSystemKind,
        };
        use ox_ontology::ir::{NodeTypeDef, PropertyDef};
        use ox_ontology::value_set::{
            IncludeMode, ValueSetDef, ValueSetId, ValueSetIncludeRule, ValueSetSelector,
        };

        let cs_id = CsId::new("cs-2024");
        let cs_target = CsId::new("cs-2026");
        let vs_id = ValueSetId::new("vs-stock-status");
        let cm = concept_map();
        let cm_id = cm.id.clone();

        let mut ir = OntologyIR::new(
            "ont".into(),
            "Test".into(),
            LocalizedText::default(),
            1,
            vec![NodeTypeDef {
                id: "n-stock".into(),
                label: GraphLabel::new("Stock").expect("label"),
                description: LocalizedText::default(),
                properties: vec![PropertyDef {
                    id: "p-status".into(),
                    name: pk("status"),
                    property_type: PropertyType::String,
                    nullable: false,
                    bindings: vec![ox_ontology::PropertyBinding::value_set(vs_id.clone())
                    .with_concept_map(cm_id.clone())],
                    ..Default::default()
                }],
                constraints: vec![],
                ..Default::default()
            }],
            vec![],
            vec![],
        );

        let make_cs = |id: CsId, name: &str| CodeSystemDef {
            id,
            name: name.into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            kind: CodeSystemKind::Internal,
            uri: None,
            hierarchical: false,
            deprecated_at: None,
            replaced_by_id: None,
            codes: Vec::new(),
        };
        ir.add_code_system(make_cs(cs_id.clone(), "krx-2024")).unwrap();
        ir.add_code_system(make_cs(cs_target, "krx-2026")).unwrap();
        ir.add_value_set(ValueSetDef {
            id: vs_id,
            name: "stock_status".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            composition: vec![ValueSetIncludeRule {
                mode: IncludeMode::Include,
                system_id: cs_id,
                selector: ValueSetSelector::All,
            }],
        })
        .unwrap();
        ir.add_concept_map(cm).unwrap();

        let q = build_match_query("s", "A001");
        let table = build_translation_table_for_query(&q, &ir);
        let (entry, _) = table
            .get(&(vn("s"), pk("status")))
            .expect("explicit binding id resolves");
        assert_eq!(entry.id, cm_id);
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

    // -----------------------------------------------------------------
    // Mutate / Analytics / PathFind literal translation
    // -----------------------------------------------------------------
    //
    // Each test pins a distinct write surface so a `MERGE (s:Stock
    // {status: "A001"})` against an old vocabulary cannot land in
    // the current storage unrewritten.

    #[test]
    fn create_node_property_assignment_is_translated() {
        use ox_query_ir::query::{MutateOp, PropertyAssignment};
        let cm = concept_map();
        let q = QueryIR {
            schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Mutate {
                context: None,
                operations: vec![MutateOp::CreateNode {
                    variable: vn("s"),
                    label: GraphLabel::new("Stock").unwrap(),
                    properties: vec![PropertyAssignment {
                        property: pk("status"),
                        value: Expr::Literal {
                            value: PropertyValue::String("A001".into()),
                        },
                    }],
                }],
                returning: Vec::new(),
            },
            limit: None,
            skip: None,
            order_by: Vec::new(),
            as_of: None,
        };
        let (rewritten, report) = rewrite_concept_map_values(q, &table(&cm, "s"));
        assert_eq!(report.translations.len(), 1, "create write must rewrite");
        let QueryOp::Mutate { operations, .. } = rewritten.operation else {
            panic!("expected Mutate");
        };
        let MutateOp::CreateNode { properties, .. } = &operations[0] else {
            panic!("expected CreateNode");
        };
        let Expr::Literal {
            value: PropertyValue::String(s),
        } = &properties[0].value
        else {
            panic!("expected string literal");
        };
        assert_eq!(s, "NEWA001", "v2024 code must be translated to v2026");
    }

    #[test]
    fn merge_node_match_and_on_create_assignments_are_translated() {
        use ox_query_ir::query::{MutateOp, PropertyAssignment};
        let cm = concept_map();
        let q = QueryIR {
            schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Mutate {
                context: None,
                operations: vec![MutateOp::MergeNode {
                    variable: vn("s"),
                    label: GraphLabel::new("Stock").unwrap(),
                    match_properties: vec![PropertyAssignment {
                        property: pk("status"),
                        value: Expr::Literal {
                            value: PropertyValue::String("A001".into()),
                        },
                    }],
                    on_create: vec![PropertyAssignment {
                        property: pk("status"),
                        value: Expr::Literal {
                            value: PropertyValue::String("A001".into()),
                        },
                    }],
                    on_match: Vec::new(),
                }],
                returning: Vec::new(),
            },
            limit: None,
            skip: None,
            order_by: Vec::new(),
            as_of: None,
        };
        let (rewritten, report) = rewrite_concept_map_values(q, &table(&cm, "s"));
        assert_eq!(
            report.translations.len(),
            2,
            "match + on_create both rewrite"
        );
        let QueryOp::Mutate { operations, .. } = rewritten.operation else {
            panic!("expected Mutate");
        };
        let MutateOp::MergeNode {
            match_properties,
            on_create,
            ..
        } = &operations[0]
        else {
            panic!("expected MergeNode");
        };
        let Expr::Literal {
            value: PropertyValue::String(m),
        } = &match_properties[0].value
        else {
            panic!("match value");
        };
        let Expr::Literal {
            value: PropertyValue::String(c),
        } = &on_create[0].value
        else {
            panic!("on_create value");
        };
        assert_eq!(m, "NEWA001");
        assert_eq!(c, "NEWA001");
    }

    #[test]
    fn set_property_value_is_translated() {
        use ox_query_ir::query::MutateOp;
        let cm = concept_map();
        let q = QueryIR {
            schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Mutate {
                context: None,
                operations: vec![MutateOp::SetProperty {
                    variable: vn("s"),
                    property: pk("status"),
                    value: Expr::Literal {
                        value: PropertyValue::String("A001".into()),
                    },
                }],
                returning: Vec::new(),
            },
            limit: None,
            skip: None,
            order_by: Vec::new(),
            as_of: None,
        };
        let (rewritten, report) = rewrite_concept_map_values(q, &table(&cm, "s"));
        assert_eq!(report.translations.len(), 1);
        let QueryOp::Mutate { operations, .. } = rewritten.operation else {
            panic!("expected Mutate");
        };
        let MutateOp::SetProperty { value, .. } = &operations[0] else {
            panic!("expected SetProperty");
        };
        let Expr::Literal {
            value: PropertyValue::String(s),
        } = value
        else {
            panic!("string literal");
        };
        assert_eq!(s, "NEWA001");
    }

    #[test]
    fn pathfind_endpoint_filter_literal_is_translated() {
        use ox_core::types::Direction;
        use ox_query_ir::query::{NodeRef, PathAlgorithm, PropertyFilter as PF};
        let cm = concept_map();
        let q = QueryIR {
            schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::PathFind {
                start: NodeRef {
                    variable: vn("s"),
                    label: Some(GraphLabel::new("Stock").unwrap()),
                    property_filters: vec![PF {
                        property: pk("status"),
                        value: Expr::Literal {
                            value: PropertyValue::String("A001".into()),
                        },
                    }],
                },
                end: NodeRef {
                    variable: vn("t"),
                    label: Some(GraphLabel::new("Stock").unwrap()),
                    property_filters: Vec::new(),
                },
                edge_types: Vec::new(),
                direction: Direction::Outgoing,
                max_depth: None,
                algorithm: PathAlgorithm::ShortestPath,
            },
            limit: None,
            skip: None,
            order_by: Vec::new(),
            as_of: None,
        };
        let (rewritten, report) = rewrite_concept_map_values(q, &table(&cm, "s"));
        assert_eq!(report.translations.len(), 1, "endpoint filter rewrites");
        let QueryOp::PathFind { start, .. } = rewritten.operation else {
            panic!("expected PathFind");
        };
        let Expr::Literal {
            value: PropertyValue::String(s),
        } = &start.property_filters[0].value
        else {
            panic!("string literal");
        };
        assert_eq!(s, "NEWA001");
    }

    #[test]
    fn analytics_subgraph_filter_recurses_into_match() {
        use ox_query_ir::query::{AnalyticsSource, GraphAlgorithm};
        let cm = concept_map();
        let inner = build_match_query("s", "A001");
        let q = QueryIR {
            schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Analytics {
                algorithm: GraphAlgorithm::PageRank,
                source: AnalyticsSource::Subgraph {
                    filter: Box::new(inner.operation),
                },
                params: Default::default(),
                projections: Vec::new(),
            },
            limit: None,
            skip: None,
            order_by: Vec::new(),
            as_of: None,
        };
        let (rewritten, report) = rewrite_concept_map_values(q, &table(&cm, "s"));
        assert_eq!(
            report.translations.len(),
            1,
            "subgraph filter inherits Match's literal-rewrite path"
        );
        let QueryOp::Analytics { source, .. } = rewritten.operation else {
            panic!("expected Analytics");
        };
        let AnalyticsSource::Subgraph { filter } = source else {
            panic!("expected Subgraph");
        };
        let QueryOp::Match { patterns, .. } = *filter else {
            panic!("expected Match inside Subgraph");
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
            panic!("string literal");
        };
        assert_eq!(s, "NEWA001");
    }

    #[test]
    fn mutate_context_match_is_recursed() {
        use ox_query_ir::query::MutateOp;
        // Mutate with a leading MATCH that itself carries a translatable
        // literal in a property filter — the recursion into `context`
        // must apply the same rewriter the top-level Match path does.
        let cm = concept_map();
        let leading_match = QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn("s"),
                label: Some(GraphLabel::new("Stock").unwrap()),
                property_filters: vec![PropertyFilter {
                    property: pk("status"),
                    value: Expr::Literal {
                        value: PropertyValue::String("A001".into()),
                    },
                }],
            }],
            filter: None,
            projections: vec![Projection::Variable {
                variable: vn("s"),
                alias: None,
            }],
            optional: false,
            group_by: Vec::new(),
        };
        let q = QueryIR {
            schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Mutate {
                context: Some(Box::new(leading_match)),
                operations: vec![MutateOp::SetProperty {
                    variable: vn("s"),
                    property: pk("status"),
                    value: Expr::Literal {
                        value: PropertyValue::String("A001".into()),
                    },
                }],
                returning: Vec::new(),
            },
            limit: None,
            skip: None,
            order_by: Vec::new(),
            as_of: None,
        };
        let (_, report) = rewrite_concept_map_values(q, &table(&cm, "s"));
        // 1 from context Match's property_filter, 1 from SetProperty.
        assert_eq!(
            report.translations.len(),
            2,
            "context Match + outer SetProperty both translate"
        );
    }
}
