//! Π-3 — `QueryProvenance` assembly.
//!
//! Every surface that returns a `QueryResult` populates
//! `metadata.provenance` so downstream consumers (admin UI "response
//! basis" panel, LLM follow-up reasoning) can see which ontology
//! version produced the numbers, which sources were read, which
//! types participated, and a compact description of the filter
//! clause. The fields on [`QueryProvenance`] are all optional so
//! partial population (e.g. a Cypher runtime that doesn't know
//! which federation `source_id` was touched) still emits a useful
//! record.
//!
//! This module is the single call site for assembling provenance —
//! routes / runtimes / planners pass in what they know, and the
//! helper stitches the rest. Keeping it in ox-compiler (which
//! already depends on both ox-query-ir and ox-ontology) avoids
//! pushing either of those crates into a direct dependency with
//! the other.

use ox_ontology::ir::OntologyIR;
use ox_query_ir::query::{
    Expr, GraphPattern, QueryIR, QueryOp, QueryProvenance,
};

/// Inputs for assembling a [`QueryProvenance`] at the API route
/// boundary. All fields are optional so a runtime path that
/// doesn't know (say) the federation source set still produces a
/// useful record.
#[derive(Debug, Default, Clone)]
pub struct ProvenanceContext<'a> {
    /// `ontologies.id` — the identity uuid the query was executed
    /// against. Serialized as the stringified uuid.
    pub ontology_id: Option<String>,
    /// `ontology_version_snapshots.version` — the free-form version
    /// tag (usually an integer counter or semver).
    pub ontology_version: Option<String>,
    /// Business-time anchor the temporal rewriter used. Captured
    /// before the rewrite consumes the field.
    pub as_of: Option<chrono::DateTime<chrono::Utc>>,
    /// Federation sources touched by the plan. Cypher path leaves
    /// this empty.
    pub source_ids: Vec<String>,
    /// Optional ontology handle so the helper can resolve node /
    /// edge labels into type ids. Without this the `type_ids`
    /// field stays empty — label-only provenance would be
    /// misleading since label renames across versions would not
    /// resolve.
    pub ontology: Option<&'a OntologyIR>,
}

/// Build a `QueryProvenance` record for `query` under `ctx`. The
/// returned value is ready to drop into `QueryMetadata.provenance`.
///
/// `type_ids` and (when the caller opts in via a non-empty
/// `ontology`) `source_ids` are derived from the IR + ontology
/// object/link mappings. Any `source_ids` pre-populated on the
/// context (e.g. a federation resolver snapshot for observability
/// fallback) are merged with the IR-derived set, dedup-preserving.
pub fn build_provenance(query: &QueryIR, ctx: &ProvenanceContext<'_>) -> QueryProvenance {
    let (type_ids, derived_source_ids, filter_descs) = walk_op(&query.operation, ctx.ontology);
    let filter_summary = summarize_filters(&filter_descs);

    let mut source_ids = ctx.source_ids.clone();
    source_ids.extend(derived_source_ids);
    dedup_preserving_order(&mut source_ids);

    // Registry version stamp — same ontology, different glossary /
    // value-set / concept-map revisions would produce different
    // user-visible results. Capturing the hashes lets the UI re-run
    // the query at the exact registry state that produced the
    // original answer.
    let registry_versions = ctx
        .ontology
        .map(registry_version_hashes)
        .unwrap_or_default();

    QueryProvenance {
        ontology_id: ctx.ontology_id.clone(),
        ontology_version: ctx.ontology_version.clone(),
        as_of: ctx.as_of,
        source_ids,
        type_ids,
        filter_summary,
        registry_versions,
    }
}

/// Deterministic fingerprint for every registry collection on an
/// `OntologyIR`. A stable short name keys into a
/// `BTreeMap<String, String>` so the serialised shape is both
/// diff-friendly and self-documenting.
fn registry_version_hashes(
    ir: &OntologyIR,
) -> std::collections::BTreeMap<String, String> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn fingerprint<T: serde::Serialize>(values: &[T]) -> String {
        let bytes = serde_json::to_vec(values).unwrap_or_default();
        let mut h = DefaultHasher::new();
        bytes.hash(&mut h);
        format!("{:016x}", h.finish())
    }

    let mut out = std::collections::BTreeMap::new();
    out.insert("code_systems".into(), fingerprint(ir.code_systems()));
    out.insert("glossary".into(), fingerprint(ir.glossary()));
    out.insert("value_sets".into(), fingerprint(ir.value_sets()));
    out.insert(
        "notation_patterns".into(),
        fingerprint(ir.notation_patterns()),
    );
    out.insert("concept_maps".into(), fingerprint(ir.concept_maps()));
    out.insert(
        "value_range_sets".into(),
        fingerprint(ir.value_range_sets()),
    );
    out.insert("rules".into(), fingerprint(ir.rules()));
    out
}

/// Walk the query operation and collect (a) the set of node / edge
/// type ids referenced, (b) the set of ontology-declared
/// `source_id`s reachable by the query's labels (empty when no
/// ontology is supplied), and (c) a flat list of short filter
/// descriptions for the summary.
///
/// Label-to-source-id resolution goes through
/// `ObjectMappingDef.source_id` for node labels and
/// `LinkMappingDef.{source_endpoint, target_endpoint,
/// kind.bridge_relation}.source_id` for edge labels. This gives the
/// Π-3 panel the narrowest "sources reachable by this query" set
/// the ontology can declare — strictly tighter than "every adapter
/// registered in the workspace" and still pure (no LogicalPlan
/// inspection required).
fn walk_op(
    op: &QueryOp,
    ontology: Option<&OntologyIR>,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut type_ids = Vec::new();
    let mut source_ids = Vec::new();
    let mut filters = Vec::new();
    collect(op, ontology, &mut type_ids, &mut source_ids, &mut filters);
    dedup_preserving_order(&mut type_ids);
    dedup_preserving_order(&mut source_ids);
    (type_ids, source_ids, filters)
}

fn collect(
    op: &QueryOp,
    ontology: Option<&OntologyIR>,
    type_ids: &mut Vec<String>,
    source_ids: &mut Vec<String>,
    filters: &mut Vec<String>,
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
                collect_pattern(p, ontology, type_ids, source_ids, filters);
            }
            if let Some(expr) = filter {
                filters.push(describe_expr(expr));
            }
        }
        QueryOp::PathFind {
            edge_types,
            start,
            end,
            ..
        } => {
            if let Some(ont) = ontology {
                for label in edge_types {
                    push_edge_type_id(ont, label.as_str(), type_ids);
                    push_edge_source_ids(ont, label.as_str(), source_ids);
                }
                if let Some(l) = &start.label {
                    push_node_type_id(ont, l.as_str(), type_ids);
                    push_node_source_ids(ont, l.as_str(), source_ids);
                }
                if let Some(l) = &end.label {
                    push_node_type_id(ont, l.as_str(), type_ids);
                    push_node_source_ids(ont, l.as_str(), source_ids);
                }
            }
        }
        QueryOp::Aggregate { source, having, .. } => {
            collect(&source.operation, ontology, type_ids, source_ids, filters);
            if let Some(expr) = having {
                filters.push(format!("having({})", describe_expr(expr)));
            }
        }
        QueryOp::Union { queries, .. } => {
            for q in queries {
                collect(&q.operation, ontology, type_ids, source_ids, filters);
            }
        }
        QueryOp::Chain { steps } => {
            for step in steps {
                collect_chain_step(step, ontology, type_ids, source_ids, filters);
            }
        }
        QueryOp::CallSubquery { inner, .. } => {
            collect(&inner.operation, ontology, type_ids, source_ids, filters);
        }
        QueryOp::Mutate { context, .. } => {
            if let Some(inner) = context {
                collect(inner.as_ref(), ontology, type_ids, source_ids, filters);
            }
        }
        QueryOp::Analytics { .. } => {}
    }
}

fn collect_chain_step(
    step: &ox_query_ir::query::ChainStep,
    ontology: Option<&OntologyIR>,
    type_ids: &mut Vec<String>,
    source_ids: &mut Vec<String>,
    filters: &mut Vec<String>,
) {
    collect(&step.operation, ontology, type_ids, source_ids, filters);
}

fn collect_pattern(
    pattern: &GraphPattern,
    ontology: Option<&OntologyIR>,
    type_ids: &mut Vec<String>,
    source_ids: &mut Vec<String>,
    filters: &mut Vec<String>,
) {
    use ox_query_ir::query::PathElement;
    match pattern {
        GraphPattern::Node {
            label,
            property_filters,
            ..
        } => {
            if let (Some(ont), Some(l)) = (ontology, label.as_ref()) {
                push_node_type_id(ont, l.as_str(), type_ids);
                push_node_source_ids(ont, l.as_str(), source_ids);
            }
            for pf in property_filters {
                filters.push(describe_property_filter(pf));
            }
        }
        GraphPattern::Relationship {
            label,
            property_filters,
            ..
        } => {
            if let (Some(ont), Some(l)) = (ontology, label.as_ref()) {
                push_edge_type_id(ont, l.as_str(), type_ids);
                push_edge_source_ids(ont, l.as_str(), source_ids);
            }
            for pf in property_filters {
                filters.push(describe_property_filter(pf));
            }
        }
        GraphPattern::Path { elements } => {
            for el in elements {
                match el {
                    PathElement::Node { label, .. } => {
                        if let (Some(ont), Some(l)) = (ontology, label.as_ref()) {
                            push_node_type_id(ont, l.as_str(), type_ids);
                            push_node_source_ids(ont, l.as_str(), source_ids);
                        }
                    }
                    PathElement::Edge { label, .. } => {
                        if let (Some(ont), Some(l)) = (ontology, label.as_ref()) {
                            push_edge_type_id(ont, l.as_str(), type_ids);
                            push_edge_source_ids(ont, l.as_str(), source_ids);
                        }
                    }
                }
            }
        }
    }
}

fn push_node_type_id(ontology: &OntologyIR, label: &str, out: &mut Vec<String>) {
    if let Some(node) = ontology.node_by_label(label) {
        out.push(node.id.to_string());
    }
}

fn push_edge_type_id(ontology: &OntologyIR, label: &str, out: &mut Vec<String>) {
    if let Some(edge) = ontology
        .edge_types()
        .iter()
        .find(|e| e.label.as_str() == label)
    {
        out.push(edge.id.to_string());
    }
}

/// Resolve a node label to its object mappings and push every
/// declared `source_id`. A node with multiple mappings (multi-source
/// ontologies) contributes every source the planner could choose
/// from — conservative but still strictly scoped to what the
/// ontology says this label can reach.
fn push_node_source_ids(ontology: &OntologyIR, label: &str, out: &mut Vec<String>) {
    let Some(node) = ontology.node_by_label(label) else {
        return;
    };
    for mapping in ontology.object_mappings() {
        if mapping.node_type_id == node.id {
            out.push(mapping.source_id.to_string());
        }
    }
}

/// Resolve an edge label to its link mappings and push every
/// declared endpoint + bridge source id. Federated links contribute
/// two sources (source + target); Bridge adds a third when the
/// bridge relation lives in yet another source.
fn push_edge_source_ids(ontology: &OntologyIR, label: &str, out: &mut Vec<String>) {
    use ox_ontology::mapping::LinkMappingKind;
    let Some(edge) = ontology
        .edge_types()
        .iter()
        .find(|e| e.label.as_str() == label)
    else {
        return;
    };
    for mapping in ontology.link_mappings() {
        if mapping.edge_type_id != edge.id {
            continue;
        }
        out.push(mapping.source_endpoint.source_id.to_string());
        out.push(mapping.target_endpoint.source_id.to_string());
        if let LinkMappingKind::Bridge { bridge_relation, .. } = &mapping.kind {
            out.push(bridge_relation.source_id.to_string());
        }
    }
}

fn dedup_preserving_order(v: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    v.retain(|s| seen.insert(s.clone()));
}

fn describe_property_filter(pf: &ox_query_ir::query::PropertyFilter) -> String {
    format!("{} = {}", pf.property.as_str(), describe_expr(&pf.value))
}

/// Short human description of an expression — used inline in the
/// filter_summary string. Keeps structural shape visible (`and`,
/// `or`, `not`) without dumping the full AST; literals are
/// summarised by type.
fn describe_expr(expr: &Expr) -> String {
    use ox_query_ir::query::{ComparisonOp, LogicalOp, StringOp};
    match expr {
        Expr::Literal { value } => literal_summary(value),
        Expr::Param { name } => format!("${name}"),
        Expr::Property {
            variable,
            field: Some(field),
        } => format!("{}.{}", variable.as_str(), field.as_str()),
        Expr::Property {
            variable,
            field: None,
        } => variable.as_str().to_string(),
        Expr::Comparison { op, left, right } => {
            let op_str = match op {
                ComparisonOp::Eq => "=",
                ComparisonOp::Neq => "!=",
                ComparisonOp::Lt => "<",
                ComparisonOp::Lte => "<=",
                ComparisonOp::Gt => ">",
                ComparisonOp::Gte => ">=",
            };
            format!("({} {} {})", describe_expr(left), op_str, describe_expr(right))
        }
        Expr::Logical { op, left, right } => {
            let op_str = match op {
                LogicalOp::And => "AND",
                LogicalOp::Or => "OR",
                LogicalOp::Xor => "XOR",
            };
            format!("({} {op_str} {})", describe_expr(left), describe_expr(right))
        }
        Expr::Not { inner } => format!("NOT {}", describe_expr(inner)),
        Expr::In { expr, values } => {
            let items: Vec<String> = values.iter().map(literal_summary).collect();
            format!("{} IN [{}]", describe_expr(expr), items.join(", "))
        }
        Expr::IsNull { expr, negated } => {
            if *negated {
                format!("{} IS NOT NULL", describe_expr(expr))
            } else {
                format!("{} IS NULL", describe_expr(expr))
            }
        }
        Expr::StringOp { left, op, right } => {
            let kw = match op {
                StringOp::StartsWith => "STARTS WITH",
                StringOp::EndsWith => "ENDS WITH",
                StringOp::Contains => "CONTAINS",
                StringOp::Regex => "=~",
            };
            format!("{} {kw} {}", describe_expr(left), describe_expr(right))
        }
        Expr::FunctionCall { function, .. } => format!("{function:?}(…)"),
        Expr::Exists { .. } => "EXISTS {…}".to_string(),
        Expr::Case { .. } => "CASE".to_string(),
        Expr::Subquery { .. } => "subquery(…)".to_string(),
    }
}

fn literal_summary(value: &ox_core::types::PropertyValue) -> String {
    use ox_core::types::PropertyValue;
    match value {
        PropertyValue::String(s) if s.len() > 24 => format!("\"{}…\"", &s[..24]),
        PropertyValue::String(s) => format!("\"{s}\""),
        PropertyValue::Int(n) => n.to_string(),
        PropertyValue::Float(f) => f.to_string(),
        PropertyValue::Bool(b) => b.to_string(),
        PropertyValue::Null => "null".to_string(),
        _ => "<value>".to_string(),
    }
}

fn summarize_filters(filters: &[String]) -> Option<String> {
    if filters.is_empty() {
        None
    } else {
        // Cap length — a filter summary is a *signal*, not a full
        // AST dump. The admin UI can click through to the query_ir
        // for the authoritative shape.
        let joined = filters.join("; ");
        if joined.len() > 240 {
            Some(format!("{}…", &joined[..240]))
        } else {
            Some(joined)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::graph_label::GraphLabel;
    use ox_core::i18n::LocalizedText;
    use ox_core::property_key::PropertyKey;
    use ox_core::types::PropertyValue;
    use ox_core::variable_name::VariableName;
    use ox_ontology::ir::{NodeTypeDef, OntologyIR, OntologyVersion};
    use ox_query_ir::query::{
        GraphPattern, PropertyFilter, QUERY_IR_SCHEMA_VERSION, QueryIR, QueryOp,
    };

    fn vn(s: &str) -> VariableName {
        VariableName::new(s).unwrap()
    }

    fn gl(s: &str) -> GraphLabel {
        GraphLabel::new(s).unwrap()
    }

    fn minimal_ontology_with_nodes(labels: &[&str]) -> OntologyIR {
        let nodes = labels
            .iter()
            .enumerate()
            .map(|(i, l)| NodeTypeDef {
                id: format!("nt_{i}").into(),
                label: gl(l),
                description: LocalizedText::default(),
                properties: vec![],
                constraints: vec![],
                ..Default::default()
            })
            .collect();
        OntologyIR::new(
            "ont-test".into(),
            "Prov Test".into(),
            LocalizedText::default(),
            OntologyVersion {
                number: 1,
                valid_from: None,
                valid_to: None,
                committed_by: None,
                commit_message: None,
            },
            nodes,
            vec![],
            vec![],
        )
    }

    #[test]
    fn bare_match_with_no_ontology_populates_scalar_fields() {
        let query = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("a"),
                    label: Some(gl("A")),
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
        };
        let ctx = ProvenanceContext {
            ontology_id: Some("ont-id".into()),
            ontology_version: Some("3".into()),
            as_of: None,
            source_ids: vec!["postgres://orders".into()],
            ontology: None,
        };
        let prov = build_provenance(&query, &ctx);
        assert_eq!(prov.ontology_id.as_deref(), Some("ont-id"));
        assert_eq!(prov.ontology_version.as_deref(), Some("3"));
        assert_eq!(prov.source_ids, vec!["postgres://orders"]);
        assert!(prov.type_ids.is_empty(), "no ontology → no type ids");
        assert!(prov.filter_summary.is_none());
    }

    #[test]
    fn node_labels_resolve_to_type_ids() {
        let ontology = minimal_ontology_with_nodes(&["A", "B"]);
        let query = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![
                    GraphPattern::Node {
                        variable: vn("a"),
                        label: Some(gl("A")),
                        property_filters: vec![],
                    },
                    GraphPattern::Node {
                        variable: vn("b"),
                        label: Some(gl("B")),
                        property_filters: vec![],
                    },
                ],
                filter: None,
                projections: vec![],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
            as_of: None,
        };
        let ctx = ProvenanceContext {
            ontology: Some(&ontology),
            ..Default::default()
        };
        let prov = build_provenance(&query, &ctx);
        assert_eq!(prov.type_ids, vec!["nt_0".to_string(), "nt_1".to_string()]);
    }

    /// Node labels backed by ObjectMappingDef surface their
    /// `source_id`. Covers the common single-source case.
    #[test]
    fn node_label_resolves_to_object_mapping_source_id() {
        use ox_ontology::mapping::ObjectMappingDef;

        let mut ontology = minimal_ontology_with_nodes(&["Customer"]);
        ontology
            .add_object_mapping(
                ObjectMappingDef::new("om_customer_pg", "nt_0", "pg-main", "customers"),
            )
            .expect("attach ObjectMapping");

        let query = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("c"),
                    label: Some(gl("Customer")),
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
        };
        let ctx = ProvenanceContext {
            ontology: Some(&ontology),
            ..Default::default()
        };
        let prov = build_provenance(&query, &ctx);
        assert_eq!(prov.source_ids, vec!["pg-main".to_string()]);
    }

    /// A node label with two ObjectMappingDefs (multi-source) yields
    /// both source_ids. Demonstrates the "conservative but strictly
    /// scoped to what the ontology declares" semantic the Π-3 panel
    /// relies on.
    #[test]
    fn multi_mapping_node_label_yields_every_declared_source() {
        use ox_ontology::mapping::ObjectMappingDef;

        let mut ontology = minimal_ontology_with_nodes(&["Customer"]);
        for (id, src) in [("om_pg", "pg-main"), ("om_dw", "snowflake-dw")] {
            ontology
                .add_object_mapping(ObjectMappingDef::new(id, "nt_0", src, "customers"))
                .expect("attach ObjectMapping");
        }

        let query = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("c"),
                    label: Some(gl("Customer")),
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
        };
        let ctx = ProvenanceContext {
            ontology: Some(&ontology),
            ..Default::default()
        };
        let prov = build_provenance(&query, &ctx);
        assert_eq!(
            prov.source_ids,
            vec!["pg-main".to_string(), "snowflake-dw".to_string()],
            "both mappings contribute, in declaration order",
        );
    }

    /// Caller-supplied `ctx.source_ids` merge with the IR-derived
    /// set. Used when a federation caller wants to include the
    /// workspace-adapter snapshot as a fallback signal alongside
    /// the ontology-declared reach.
    #[test]
    fn context_source_ids_merge_with_ontology_derived() {
        use ox_ontology::mapping::ObjectMappingDef;

        let mut ontology = minimal_ontology_with_nodes(&["A"]);
        ontology
            .add_object_mapping(ObjectMappingDef::new("om_a", "nt_0", "pg-main", "a"))
            .expect("attach ObjectMapping");

        let query = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("a"),
                    label: Some(gl("A")),
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
        };
        let ctx = ProvenanceContext {
            source_ids: vec!["extra-src".into(), "pg-main".into()],
            ontology: Some(&ontology),
            ..Default::default()
        };
        let prov = build_provenance(&query, &ctx);
        assert_eq!(
            prov.source_ids,
            vec!["extra-src".to_string(), "pg-main".to_string()],
            "context entries come first; dup with IR-derived drops on merge",
        );
    }

    #[test]
    fn inline_property_filter_contributes_to_summary() {
        let query = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("a"),
                    label: Some(gl("A")),
                    property_filters: vec![PropertyFilter {
                        property: PropertyKey::new("status").unwrap(),
                        value: Expr::Literal {
                            value: PropertyValue::String("ACTIVE".into()),
                        },
                    }],
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
        };
        let prov = build_provenance(&query, &ProvenanceContext::default());
        let summary = prov.filter_summary.expect("filter summary set");
        assert!(summary.contains("status"), "summary mentions property: {summary}");
        assert!(summary.contains("ACTIVE"), "summary mentions literal: {summary}");
    }
}
