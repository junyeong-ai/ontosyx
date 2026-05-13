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

use ox_ontology::ir::{NodeTypeDef, OntologyIR, PropertyDef};
use ox_ontology::mapping::{
    ObjectMappingDef, PropertyLocation, PropertyMappingDef, PropertyTransform,
};
use ox_query_ir::query::{
    ColumnLineage, Expr, GraphPattern, Projection, QueryIR, QueryOp, QueryProvenance,
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

    let column_lineage = ctx
        .ontology
        .map(|ont| derive_column_lineage(query, ont))
        .unwrap_or_default();

    QueryProvenance {
        ontology_id: ctx.ontology_id.clone(),
        ontology_version: ctx.ontology_version.clone(),
        as_of: ctx.as_of,
        source_ids,
        type_ids,
        filter_summary,
        registry_versions,
        column_lineage,
        // Routing reason is stamped by the agent's `query_graph`
        // tool on the response side, not here. The compiler runs
        // before dispatch picks a backend.
        routing: None,
    }
}

/// Walk every `QueryOp::Match` in the query tree and resolve each
/// `Projection::Field` (or projected `AllProperties`) back to the
/// physical source column it reads from, by way of the ontology's
/// `ObjectMappingDef.property_mappings`. Returns one
/// [`ColumnLineage`] per resolvable output column; projections that
/// reference unknown labels / unmapped properties / non-`Column`
/// locations (json-path, derived expressions) are silently skipped
/// — lineage is best-effort attribution, never load-bearing.
///
/// Industry reference: dbt `column_level_lineage`, BigQuery
/// `INFORMATION_SCHEMA.COLUMN_LINEAGE`, Snowflake `ACCESS_HISTORY
/// .OBJECTS_MODIFIED.columns`, Palantir Foundry "data lineage" view.
/// All resolve `(query column) → (source table.column)` at
/// compile/response time so the consumer can answer "where did this
/// cell come from?" without walking the planner's internals.
pub fn derive_column_lineage(query: &QueryIR, ontology: &OntologyIR) -> Vec<ColumnLineage> {
    let mut out = Vec::new();
    collect_column_lineage(&query.operation, ontology, &mut out);
    out
}

/// Recursive walker — every `QueryOp::Match` projects, plus the
/// composite operators (`Chain`, `Union`, `CallSubquery`,
/// `Aggregate`) carry inner `QueryOp` nodes that may project too.
fn collect_column_lineage(op: &QueryOp, ontology: &OntologyIR, out: &mut Vec<ColumnLineage>) {
    match op {
        QueryOp::Match {
            patterns,
            projections,
            ..
        } => {
            // Variable → label index for the patterns in scope. A
            // variable bound by `Node { variable, label }` is what
            // gives a `Projection::Field { variable, field }` its
            // grounding — without a label we cannot resolve the
            // node type and therefore the physical mapping.
            let mut variable_label: std::collections::HashMap<&str, &str> =
                std::collections::HashMap::new();
            for p in patterns {
                if let GraphPattern::Node {
                    variable,
                    label: Some(label),
                    ..
                } = p
                {
                    variable_label.insert(variable.as_str(), label.as_str());
                }
            }

            for projection in projections {
                emit_projection_lineage(projection, &variable_label, ontology, None, out);
            }
        }
        QueryOp::Chain { steps } => {
            for s in steps {
                collect_column_lineage(&s.operation, ontology, out);
            }
        }
        QueryOp::Union { queries, .. } => {
            for q in queries {
                collect_column_lineage(&q.operation, ontology, out);
            }
        }
        QueryOp::CallSubquery { inner, .. } => {
            collect_column_lineage(&inner.operation, ontology, out);
        }
        QueryOp::Aggregate { source, .. } => {
            collect_column_lineage(&source.operation, ontology, out);
        }
        // PathFind / Mutate / Analytics / HybridSearch carry no
        // projection surface this walker can attribute to a
        // source column; future work adds them when their
        // projection shape lands. HybridSearch in particular
        // returns a ranked node list scored against a vector,
        // which has no per-column lineage to a relational
        // mapping.
        QueryOp::PathFind { .. }
        | QueryOp::Mutate { .. }
        | QueryOp::Analytics { .. }
        | QueryOp::HybridSearch { .. } => {}
    }
}

/// Emit lineage rows for one [`Projection`]. Recurses into
/// `Aggregation::argument` so `count(c.id)` carries the inner field
/// reference's lineage with an `count(...)` transform suffix.
///
/// `output_alias_override` lets the recursive aggregation case
/// substitute the aggregation's own alias for the inner projection's
/// output column.
fn emit_projection_lineage(
    projection: &Projection,
    variable_label: &std::collections::HashMap<&str, &str>,
    ontology: &OntologyIR,
    output_alias_override: Option<String>,
    out: &mut Vec<ColumnLineage>,
) {
    match projection {
        Projection::Field {
            variable,
            field,
            alias,
        } => {
            let Some(label) = variable_label.get(variable.as_str()) else {
                return;
            };
            let Some(node) = ontology.node_by_label(label) else {
                return;
            };
            let Some(prop) = node
                .properties
                .iter()
                .find(|p| p.name.as_str() == field.as_str())
            else {
                return;
            };
            let output_column = output_alias_override.unwrap_or_else(|| {
                alias
                    .clone()
                    .unwrap_or_else(|| format!("{}.{}", variable.as_str(), field.as_str()))
            });
            for_each_property_mapping(node, prop, ontology, |om, pm| {
                if let Some(row) = build_lineage_row(&output_column, om, pm) {
                    out.push(row);
                }
            });
        }
        Projection::Variable { variable, alias } => {
            emit_all_properties(
                variable.as_str(),
                alias.as_deref(),
                variable_label,
                ontology,
                out,
            );
        }
        Projection::AllProperties { variable } => {
            emit_all_properties(variable.as_str(), None, variable_label, ontology, out);
        }
        Projection::Expression { expr, alias } => {
            // Walk the expression to collect every
            // `Expr::Property { variable, field: Some(_) }` reference,
            // attribute each to its source column, and tag the lineage
            // with the expression text. Whole-variable references
            // (`Expr::Property { field: None }`, e.g. `id(n)`) point
            // at the node-identity surface — `ObjectMappingDef
            // .primary_key_columns` may be composite, so there is no
            // single column to attribute. Silent skip is the honest
            // answer; emitting a fan-out across every property would
            // be a heuristic lie.
            let expr_repr = describe_expr(expr);
            let mut refs = Vec::new();
            collect_expr_property_refs(expr, &mut refs);
            for (var, field_opt) in refs {
                let Some(field) = field_opt else { continue };
                let Some(label) = variable_label.get(var.as_str()) else {
                    continue;
                };
                let Some(node) = ontology.node_by_label(label) else {
                    continue;
                };
                let Some(prop) = node
                    .properties
                    .iter()
                    .find(|p| p.name.as_str() == field.as_str())
                else {
                    continue;
                };
                for_each_property_mapping(node, prop, ontology, |om, pm| {
                    if let Some(mut row) = build_lineage_row(alias, om, pm) {
                        row.transform = Some(compose_transform(
                            Some(&expr_repr),
                            row.transform.as_deref(),
                        ));
                        out.push(row);
                    }
                });
            }
        }
        Projection::Aggregation {
            function,
            argument,
            alias,
            distinct,
        } => {
            // Aggregations name the inner projection's source columns
            // and wrap the lineage's transform in `<FN>(...)`. A
            // wildcard aggregation (`count(*)`) has no inner argument
            // — emit a synthetic row with no source so the consumer
            // still sees the column attributed to "the whole row".
            let fn_name = agg_function_name(function);
            let Some(arg) = argument else {
                // Whole-row aggregation (`count(*)`) — sentinel "*" on
                // both sides marks "the entire scan", symmetric with
                // `source_column = "*"`. Empty would be ambiguous with
                // "ontology has no source declared".
                out.push(ColumnLineage {
                    output_column: alias.clone(),
                    source_id: "*".to_string(),
                    source_column: "*".to_string(),
                    transform: Some(format!("{fn_name}(*)")),
                });
                return;
            };
            let mut inner = Vec::new();
            emit_projection_lineage(arg, variable_label, ontology, None, &mut inner);
            for mut row in inner {
                row.output_column = alias.clone();
                let prefix = if *distinct {
                    format!("{fn_name}(DISTINCT ")
                } else {
                    format!("{fn_name}(")
                };
                let inner_repr = row
                    .transform
                    .clone()
                    .unwrap_or_else(|| row.source_column.clone());
                row.transform = Some(format!("{prefix}{inner_repr})"));
                out.push(row);
            }
        }
    }
}

/// Helper: walk every `(ObjectMappingDef, PropertyMappingDef)` pair
/// for a `(node, property)` combo. Edges that don't carry per-property
/// mappings in the IR fall outside the walker — this stays node-only.
fn for_each_property_mapping<F>(
    node: &NodeTypeDef,
    prop: &PropertyDef,
    ontology: &OntologyIR,
    mut f: F,
) where
    F: FnMut(&ObjectMappingDef, &PropertyMappingDef),
{
    for om in ontology.object_mappings() {
        if om.node_type_id != node.id {
            continue;
        }
        for pm in &om.property_mappings {
            if pm.property_id != prop.id {
                continue;
            }
            f(om, pm);
        }
    }
}

/// Emit lineage rows for every declared property of the node type
/// bound to `variable_name`. Used by `Projection::Variable` and
/// `Projection::AllProperties` — both project "every property" of the
/// node, differing only in alias semantics.
///
/// Two-tier emission preserves Progressive Disclosure: the default
/// (everything reads from one column verbatim) collapses to a single
/// row per source mapping; only properties that carry a non-trivial
/// transform (`SqlExpr`, `Derived`, `JsonPath`, `concept_map_id`,
/// or any property whose location is non-`Column`) get an individual
/// row. The aggregate row uses the `*` sentinel on both sides (mirrors
/// the `count(*)` aggregation row) so consumers can tell "scan-wide"
/// from "individual column" by shape alone.
///
/// `alias_root` (when present) prefixes the per-property output column
/// so multiple `Variable` projections on the same query don't collide
/// on a bare `name`.
fn emit_all_properties(
    variable_name: &str,
    alias_root: Option<&str>,
    variable_label: &std::collections::HashMap<&str, &str>,
    ontology: &OntologyIR,
    out: &mut Vec<ColumnLineage>,
) {
    let Some(label) = variable_label.get(variable_name) else {
        return;
    };
    let Some(node) = ontology.node_by_label(label) else {
        return;
    };

    // Group property mappings by their owning ObjectMapping so the
    // aggregate row is per-source-per-variable, not per-property.
    type MappedProperties<'a> = (
        &'a ObjectMappingDef,
        Vec<(&'a PropertyDef, &'a PropertyMappingDef)>,
    );
    let mut per_mapping: std::collections::BTreeMap<&str, MappedProperties<'_>> =
        std::collections::BTreeMap::new();
    for om in ontology.object_mappings() {
        if om.node_type_id != node.id {
            continue;
        }
        for prop in &node.properties {
            for pm in &om.property_mappings {
                if pm.property_id == prop.id {
                    per_mapping
                        .entry(om.id.as_str())
                        .or_insert_with(|| (om, Vec::new()))
                        .1
                        .push((prop, pm));
                }
            }
        }
    }

    let var_alias = alias_root.unwrap_or(variable_name);

    for (_, (om, prop_mappings)) in per_mapping {
        // Aggregate row: `<var>.* ← <relation>.*`. Shape mirrors
        // `count(*)` — sentinel `*` on both `output_column` and
        // `source_column`. The relation portion of `source_column`
        // names the mapped relation so multi-mapping nodes still
        // separate one aggregate per scan.
        out.push(ColumnLineage {
            output_column: format!("{var_alias}.*"),
            source_id: om.source_id.as_str().to_string(),
            source_column: format!("{}.*", om.relation),
            transform: None,
        });

        // Individuated rows for properties whose physical mapping
        // applies a transform — the aggregate cannot honestly carry
        // their `transform` strings, so surface them explicitly.
        for (prop, pm) in prop_mappings {
            if !mapping_is_trivial(pm) {
                let output_column = format!("{var_alias}.{}", prop.name.as_str());
                if let Some(row) = build_lineage_row(&output_column, om, pm) {
                    out.push(row);
                }
            }
        }
    }
}

/// A property mapping is "trivial" when the upstream value reaches
/// the property unchanged — `Column` location, `Identity` transform,
/// no concept-map rewrite. Trivial mappings collapse into the
/// `<var>.*` aggregate row; non-trivial ones earn an individual entry.
fn mapping_is_trivial(pm: &PropertyMappingDef) -> bool {
    matches!(pm.location, PropertyLocation::Column(_))
        && matches!(pm.transform, PropertyTransform::Identity)
        && pm.concept_map_id.is_none()
}

/// Compose a single lineage row from the ObjectMapping + PropertyMapping
/// pair. Handles both `PropertyLocation` variants (`Column`, `JsonPath`)
/// and merges the mapping's `transform` + `concept_map_id` into a
/// single human-readable transform string.
///
/// Returns `None` only when the mapping carries no usable physical
/// reference — currently never (both `Column` and `JsonPath` resolve
/// to a stable address). The Option return shape keeps the call sites
/// uniform with future variants that may legitimately decline.
fn build_lineage_row(
    output_column: &str,
    om: &ObjectMappingDef,
    pm: &PropertyMappingDef,
) -> Option<ColumnLineage> {
    let (source_column, location_transform) = match &pm.location {
        PropertyLocation::Column(col_ref) => {
            (format!("{}.{}", col_ref.relation, col_ref.column), None)
        }
        PropertyLocation::JsonPath { root_column, path } => (
            format!("{}.{}", root_column, path),
            Some(format!("json_path({}.{})", root_column, path)),
        ),
    };

    let value_transform = match &pm.transform {
        PropertyTransform::Identity => None,
        PropertyTransform::SqlExpr { expression } => Some(format!("sql({})", expression)),
        PropertyTransform::Derived { function_id } => Some(format!("derived({})", function_id)),
        PropertyTransform::Concat {
            parts,
            separator,
            skip_when_null,
        } => {
            let cols = parts
                .iter()
                .map(|c| format!("{}.{}", c.relation, c.column))
                .collect::<Vec<_>>()
                .join(", ");
            Some(format!(
                "concat([{cols}], sep={separator:?}, skip_null={skip_when_null})"
            ))
        }
    };

    let concept_map_transform = pm
        .concept_map_id
        .as_ref()
        .map(|cm| format!("concept_map({})", cm.as_str()));

    // Compose in a stable order: location → value → concept_map.
    // Layered so a JsonPath + ConceptMap binding reads
    // `concept_map(cs-2024 → 2026) ∘ json_path(address.zip)`.
    let parts: Vec<String> = [location_transform, value_transform, concept_map_transform]
        .into_iter()
        .flatten()
        .collect();
    let transform = if parts.is_empty() {
        None
    } else {
        Some(parts.join(" ∘ "))
    };

    Some(ColumnLineage {
        output_column: output_column.to_string(),
        source_id: om.source_id.as_str().to_string(),
        source_column,
        transform,
    })
}

/// Compose an outer transform with an inner transform, separating
/// with the function-composition operator. `outer` wins when `inner`
/// is empty — used by Expression projections that wrap the
/// physical-side transform with the projection-side expression.
fn compose_transform(outer: Option<&str>, inner: Option<&str>) -> String {
    match (outer, inner) {
        (Some(o), Some(i)) => format!("{} ∘ {}", o, i),
        (Some(o), None) => o.to_string(),
        (None, Some(i)) => i.to_string(),
        (None, None) => String::new(),
    }
}

/// Collect every `Expr::Property` reference in a tree of expressions.
/// Used by `emit_projection_lineage` to attribute an expression
/// projection's lineage to the source columns the expression reads.
fn collect_expr_property_refs<'a>(
    expr: &'a Expr,
    out: &mut Vec<(
        &'a ox_core::VariableName,
        Option<&'a ox_core::property_key::PropertyKey>,
    )>,
) {
    match expr {
        Expr::Property { variable, field } => {
            out.push((variable, field.as_ref()));
        }
        Expr::Comparison { left, right, .. }
        | Expr::Logical { left, right, .. }
        | Expr::StringOp { left, right, .. } => {
            collect_expr_property_refs(left, out);
            collect_expr_property_refs(right, out);
        }
        Expr::Not { inner } => collect_expr_property_refs(inner, out),
        Expr::In { expr, .. } => collect_expr_property_refs(expr, out),
        Expr::IsNull { expr, .. } => collect_expr_property_refs(expr, out),
        Expr::FunctionCall { args, .. } => {
            for arg in args {
                collect_expr_property_refs(arg, out);
            }
        }
        Expr::Case {
            operand,
            when_clauses,
            else_result,
        } => {
            if let Some(o) = operand {
                collect_expr_property_refs(o, out);
            }
            for clause in when_clauses {
                collect_expr_property_refs(&clause.condition, out);
                collect_expr_property_refs(&clause.result, out);
            }
            if let Some(d) = else_result {
                collect_expr_property_refs(d, out);
            }
        }
        Expr::Literal { .. } | Expr::Param { .. } | Expr::Exists { .. } | Expr::Subquery { .. } => {
        }
    }
}

/// Display-equivalent name of an `AggFunction`. The enum derives only
/// `Debug`; matching here keeps the user-facing transform string in
/// the canonical Cypher casing without forcing a wider Display impl.
fn agg_function_name(f: &ox_query_ir::query::AggFunction) -> &'static str {
    use ox_query_ir::query::AggFunction;
    match f {
        AggFunction::Count => "count",
        AggFunction::Sum => "sum",
        AggFunction::Avg => "avg",
        AggFunction::Min => "min",
        AggFunction::Max => "max",
        AggFunction::Collect => "collect",
        AggFunction::StdDev => "stdev",
        AggFunction::Percentile => "percentile",
        AggFunction::CollectList => "collect_list",
    }
}

/// Deterministic fingerprint for every registry collection on an
/// `OntologyIR`. A stable short name keys into a
/// `BTreeMap<String, String>` so the serialised shape is both
/// diff-friendly and self-documenting.
fn registry_version_hashes(ir: &OntologyIR) -> std::collections::BTreeMap<String, String> {
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
fn walk_op(op: &QueryOp, ontology: Option<&OntologyIR>) -> (Vec<String>, Vec<String>, Vec<String>) {
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
        QueryOp::HybridSearch { request } => {
            // The graph constraint sub-pattern can reference
            // labels — record them so the response provenance
            // names what the hybrid retrieval was scoped to.
            if let Some(constraint) = &request.graph_constraints {
                for node in &constraint.nodes {
                    if let Some(lbl) = &node.label {
                        type_ids.push(lbl.to_string());
                    }
                }
            }
        }
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
        if let LinkMappingKind::Bridge {
            bridge_relation, ..
        } = &mapping.kind
        {
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
/// summarized by type.
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
            format!(
                "({} {} {})",
                describe_expr(left),
                op_str,
                describe_expr(right)
            )
        }
        Expr::Logical { op, left, right } => {
            let op_str = match op {
                LogicalOp::And => "AND",
                LogicalOp::Or => "OR",
                LogicalOp::Xor => "XOR",
            };
            format!(
                "({} {op_str} {})",
                describe_expr(left),
                describe_expr(right)
            )
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
            .add_object_mapping(ObjectMappingDef::new(
                "om_customer_pg",
                "nt_0",
                "pg-main",
                "customers",
            ))
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
        assert!(
            summary.contains("status"),
            "summary mentions property: {summary}"
        );
        assert!(
            summary.contains("ACTIVE"),
            "summary mentions literal: {summary}"
        );
    }

    // ----- column lineage -----

    #[test]
    fn derive_column_lineage_resolves_field_projection_through_object_mapping() {
        use ox_ontology::ir::PropertyDef;
        use ox_ontology::mapping::refs::{ColumnRef, ObjectMappingId, SourceId};
        use ox_ontology::mapping::{
            CacheHintKind, ObjectMappingDef, PropertyLocation, PropertyMappingDef,
            PropertyTransform, SourceRelationKind,
        };
        use ox_query_ir::query::Projection;

        let mut ir = OntologyIR::new(
            "ont".into(),
            "Lineage".into(),
            LocalizedText::default(),
            OntologyVersion {
                number: 1,
                ..Default::default()
            },
            vec![NodeTypeDef {
                id: "nt-customer".into(),
                label: gl("Customer"),
                description: LocalizedText::default(),
                properties: vec![PropertyDef {
                    id: "p-name".into(),
                    name: PropertyKey::new("name").unwrap(),
                    property_type: ox_core::types::PropertyType::String,
                    nullable: false,
                    ..Default::default()
                }],
                constraints: vec![],
                ..Default::default()
            }],
            vec![],
            vec![],
        );

        ir.add_object_mapping(ObjectMappingDef {
            id: ObjectMappingId::new("om-customer"),
            node_type_id: "nt-customer".into(),
            source_id: SourceId::new("pg-main"),
            relation: "customers".into(),
            relation_kind: SourceRelationKind::Table,
            primary_key_columns: vec![ColumnRef::new("customers", "id")],
            row_filter: None,
            partition_columns: Vec::new(),
            workspace_scope: None,
            valid_from: None,
            valid_to: None,
            precedence: 100,
            cache_hint: CacheHintKind::default(),
            property_mappings: vec![PropertyMappingDef {
                property_id: "p-name".into(),
                property_key: PropertyKey::new("name").unwrap(),
                location: PropertyLocation::Column(ColumnRef::new("customers", "full_name")),
                transform: PropertyTransform::Identity,
                concept_map_id: None,
            }],
        })
        .expect("seed object mapping");

        let query = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("c"),
                    label: Some(gl("Customer")),
                    property_filters: Vec::new(),
                }],
                filter: None,
                projections: vec![Projection::Field {
                    variable: vn("c"),
                    field: PropertyKey::new("name").unwrap(),
                    alias: Some("display_name".into()),
                }],
                optional: false,
                group_by: Vec::new(),
            },
            limit: None,
            skip: None,
            order_by: Vec::new(),
            as_of: None,
        };

        let lineage = derive_column_lineage(&query, &ir);
        assert_eq!(lineage.len(), 1);
        let edge = &lineage[0];
        assert_eq!(edge.output_column, "display_name");
        assert_eq!(edge.source_id, "pg-main");
        assert_eq!(edge.source_column, "customers.full_name");
        assert!(edge.transform.is_none(), "no concept_map → no transform");
    }

    #[test]
    fn derive_column_lineage_skips_unlabeled_or_unmapped_projections() {
        let ir = minimal_ontology_with_nodes(&["Customer"]);
        // Variable without label binding → cannot ground to a node.
        let query = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![],
                filter: None,
                projections: vec![ox_query_ir::query::Projection::Field {
                    variable: vn("anon"),
                    field: PropertyKey::new("x").unwrap(),
                    alias: None,
                }],
                optional: false,
                group_by: Vec::new(),
            },
            limit: None,
            skip: None,
            order_by: Vec::new(),
            as_of: None,
        };
        assert!(derive_column_lineage(&query, &ir).is_empty());
    }

    // ----- Wave 8.8 — variant coverage -----
    //
    // Helper that seeds an ontology with one Customer node, two
    // properties, and an ObjectMappingDef declaring them as Column
    // / JsonPath / SqlExpr variants. Tests below pull this fixture
    // and project against it through every Projection variant.

    fn lineage_fixture() -> OntologyIR {
        use ox_ontology::ir::PropertyDef;
        use ox_ontology::mapping::refs::{ColumnRef, ObjectMappingId, SourceId};
        use ox_ontology::mapping::{
            CacheHintKind, ObjectMappingDef, PropertyMappingDef, SourceRelationKind,
        };

        let mut ir = OntologyIR::new(
            "ont".into(),
            "VariantCoverage".into(),
            LocalizedText::default(),
            OntologyVersion {
                number: 1,
                ..Default::default()
            },
            vec![NodeTypeDef {
                id: "nt-customer".into(),
                label: gl("Customer"),
                description: LocalizedText::default(),
                properties: vec![
                    PropertyDef {
                        id: "p-name".into(),
                        name: PropertyKey::new("name").unwrap(),
                        property_type: ox_core::types::PropertyType::String,
                        nullable: false,
                        ..Default::default()
                    },
                    PropertyDef {
                        id: "p-zip".into(),
                        name: PropertyKey::new("zip").unwrap(),
                        property_type: ox_core::types::PropertyType::String,
                        nullable: true,
                        ..Default::default()
                    },
                    PropertyDef {
                        id: "p-tier".into(),
                        name: PropertyKey::new("tier").unwrap(),
                        property_type: ox_core::types::PropertyType::String,
                        nullable: true,
                        ..Default::default()
                    },
                ],
                constraints: vec![],
                ..Default::default()
            }],
            vec![],
            vec![],
        );

        ir.add_object_mapping(ObjectMappingDef {
            id: ObjectMappingId::new("om-customer"),
            node_type_id: "nt-customer".into(),
            source_id: SourceId::new("pg-main"),
            relation: "customers".into(),
            relation_kind: SourceRelationKind::Table,
            primary_key_columns: vec![ColumnRef::new("customers", "id")],
            row_filter: None,
            partition_columns: Vec::new(),
            workspace_scope: None,
            valid_from: None,
            valid_to: None,
            precedence: 100,
            cache_hint: CacheHintKind::default(),
            property_mappings: vec![
                PropertyMappingDef {
                    property_id: "p-name".into(),
                    property_key: PropertyKey::new("name").unwrap(),
                    location: PropertyLocation::Column(ColumnRef::new("customers", "full_name")),
                    transform: PropertyTransform::Identity,
                    concept_map_id: None,
                },
                PropertyMappingDef {
                    property_id: "p-zip".into(),
                    property_key: PropertyKey::new("zip").unwrap(),
                    location: PropertyLocation::JsonPath {
                        root_column: "address".into(),
                        path: "postal_code".into(),
                    },
                    transform: PropertyTransform::Identity,
                    concept_map_id: None,
                },
                PropertyMappingDef {
                    property_id: "p-tier".into(),
                    property_key: PropertyKey::new("tier").unwrap(),
                    location: PropertyLocation::Column(ColumnRef::new("customers", "raw_tier")),
                    transform: PropertyTransform::SqlExpr {
                        expression: "UPPER(raw_tier)".into(),
                    },
                    concept_map_id: None,
                },
            ],
        })
        .expect("seed object mapping");
        ir
    }

    fn match_query_with_projections(projections: Vec<Projection>) -> QueryIR {
        QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("c"),
                    label: Some(gl("Customer")),
                    property_filters: Vec::new(),
                }],
                filter: None,
                projections,
                optional: false,
                group_by: Vec::new(),
            },
            limit: None,
            skip: None,
            order_by: Vec::new(),
            as_of: None,
        }
    }

    #[test]
    fn variable_projection_emits_aggregate_plus_individuated_rows_only_for_non_trivial_transforms()
    {
        // The fixture has three properties on Customer:
        //   name -> Identity Column        (trivial)
        //   zip  -> JsonPath               (non-trivial)
        //   tier -> SqlExpr (UPPER(...))   (non-trivial)
        //
        // Two-tier emission collapses the trivial mapping into the
        // single aggregate row and individuates the two non-trivial
        // ones. `name` MUST NOT have an individual row — exposing it
        // would defeat the Progressive-Disclosure design.
        let ir = lineage_fixture();
        let q = match_query_with_projections(vec![Projection::Variable {
            variable: vn("c"),
            alias: None,
        }]);
        let lineage = derive_column_lineage(&q, &ir);

        let outputs: Vec<&str> = lineage.iter().map(|l| l.output_column.as_str()).collect();
        assert!(
            outputs.contains(&"c.*"),
            "aggregate row missing: {lineage:#?}"
        );
        assert!(
            outputs.contains(&"c.zip"),
            "JsonPath property must individuate: {lineage:#?}"
        );
        assert!(
            outputs.contains(&"c.tier"),
            "SqlExpr property must individuate: {lineage:#?}"
        );
        assert!(
            !outputs.contains(&"c.name"),
            "trivial Identity mapping must collapse into aggregate, not appear standalone: {lineage:#?}",
        );
        assert_eq!(
            lineage.len(),
            3,
            "1 aggregate + 2 individuated; got {lineage:#?}"
        );

        let aggregate = lineage
            .iter()
            .find(|l| l.output_column == "c.*")
            .expect("aggregate present");
        assert_eq!(aggregate.source_column, "customers.*");
        assert_eq!(aggregate.source_id, "pg-main");
        assert!(aggregate.transform.is_none());
    }

    #[test]
    fn all_properties_projection_matches_variable_semantics() {
        let ir = lineage_fixture();
        let q = match_query_with_projections(vec![Projection::AllProperties { variable: vn("c") }]);
        let lineage = derive_column_lineage(&q, &ir);
        // Same shape contract as Projection::Variable — the difference
        // is alias semantics, not row count.
        assert_eq!(lineage.len(), 3, "{lineage:#?}");
        assert!(
            lineage.iter().any(|l| l.output_column == "c.*"),
            "{lineage:#?}",
        );
    }

    #[test]
    fn variable_projection_with_only_trivial_mappings_emits_aggregate_only() {
        // A node whose every property maps via Identity Column should
        // collapse to one aggregate row per source — no individuated
        // rows. Proves the Progressive-Disclosure default.
        use ox_ontology::ir::PropertyDef;
        use ox_ontology::mapping::refs::{ColumnRef, ObjectMappingId, SourceId};
        use ox_ontology::mapping::{
            CacheHintKind, ObjectMappingDef, PropertyMappingDef, SourceRelationKind,
        };

        let mut ir = OntologyIR::new(
            "ont".into(),
            "Trivial".into(),
            LocalizedText::default(),
            OntologyVersion {
                number: 1,
                ..Default::default()
            },
            vec![NodeTypeDef {
                id: "nt-user".into(),
                label: gl("User"),
                description: LocalizedText::default(),
                properties: vec![
                    PropertyDef {
                        id: "p-id".into(),
                        name: PropertyKey::new("id").unwrap(),
                        property_type: ox_core::types::PropertyType::Int,
                        nullable: false,
                        ..Default::default()
                    },
                    PropertyDef {
                        id: "p-email".into(),
                        name: PropertyKey::new("email").unwrap(),
                        property_type: ox_core::types::PropertyType::String,
                        nullable: false,
                        ..Default::default()
                    },
                ],
                constraints: vec![],
                ..Default::default()
            }],
            vec![],
            vec![],
        );
        ir.add_object_mapping(ObjectMappingDef {
            id: ObjectMappingId::new("om-user"),
            node_type_id: "nt-user".into(),
            source_id: SourceId::new("pg-main"),
            relation: "users".into(),
            relation_kind: SourceRelationKind::Table,
            primary_key_columns: vec![ColumnRef::new("users", "id")],
            row_filter: None,
            partition_columns: Vec::new(),
            workspace_scope: None,
            valid_from: None,
            valid_to: None,
            precedence: 100,
            cache_hint: CacheHintKind::default(),
            property_mappings: vec![
                PropertyMappingDef {
                    property_id: "p-id".into(),
                    property_key: PropertyKey::new("id").unwrap(),
                    location: PropertyLocation::Column(ColumnRef::new("users", "id")),
                    transform: PropertyTransform::Identity,
                    concept_map_id: None,
                },
                PropertyMappingDef {
                    property_id: "p-email".into(),
                    property_key: PropertyKey::new("email").unwrap(),
                    location: PropertyLocation::Column(ColumnRef::new("users", "email_addr")),
                    transform: PropertyTransform::Identity,
                    concept_map_id: None,
                },
            ],
        })
        .expect("seed");

        let q = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("u"),
                    label: Some(gl("User")),
                    property_filters: Vec::new(),
                }],
                filter: None,
                projections: vec![Projection::Variable {
                    variable: vn("u"),
                    alias: None,
                }],
                optional: false,
                group_by: Vec::new(),
            },
            limit: None,
            skip: None,
            order_by: Vec::new(),
            as_of: None,
        };
        let lineage = derive_column_lineage(&q, &ir);
        assert_eq!(lineage.len(), 1, "all-trivial must collapse: {lineage:#?}");
        assert_eq!(lineage[0].output_column, "u.*");
        assert_eq!(lineage[0].source_column, "users.*");
    }

    #[test]
    fn json_path_location_records_path_in_transform() {
        let ir = lineage_fixture();
        let q = match_query_with_projections(vec![Projection::Field {
            variable: vn("c"),
            field: PropertyKey::new("zip").unwrap(),
            alias: Some("zip_code".into()),
        }]);
        let lineage = derive_column_lineage(&q, &ir);
        assert_eq!(lineage.len(), 1);
        let row = &lineage[0];
        assert_eq!(row.output_column, "zip_code");
        assert_eq!(row.source_column, "address.postal_code");
        let t = row.transform.as_deref().unwrap_or_default();
        assert!(
            t.contains("json_path(address.postal_code)"),
            "transform should mention json_path: {t}"
        );
    }

    #[test]
    fn sql_expr_transform_records_expression_text() {
        let ir = lineage_fixture();
        let q = match_query_with_projections(vec![Projection::Field {
            variable: vn("c"),
            field: PropertyKey::new("tier").unwrap(),
            alias: None,
        }]);
        let lineage = derive_column_lineage(&q, &ir);
        assert_eq!(lineage.len(), 1);
        let t = lineage[0].transform.as_deref().unwrap_or_default();
        assert!(
            t.contains("sql(UPPER(raw_tier))"),
            "transform should carry the SQL expression: {t}"
        );
    }

    #[test]
    fn aggregation_projection_wraps_inner_lineage() {
        use ox_query_ir::query::AggFunction;

        let ir = lineage_fixture();
        let inner = Projection::Field {
            variable: vn("c"),
            field: PropertyKey::new("name").unwrap(),
            alias: None,
        };
        let q = match_query_with_projections(vec![Projection::Aggregation {
            function: AggFunction::Count,
            argument: Some(Box::new(inner)),
            alias: "name_count".into(),
            distinct: true,
        }]);
        let lineage = derive_column_lineage(&q, &ir);
        assert_eq!(lineage.len(), 1);
        let row = &lineage[0];
        assert_eq!(row.output_column, "name_count");
        assert_eq!(row.source_column, "customers.full_name");
        let t = row.transform.as_deref().unwrap_or_default();
        assert!(
            t.contains("count(DISTINCT") && t.contains("customers.full_name"),
            "transform should wrap the inner column: {t}"
        );
    }

    #[test]
    fn aggregation_wildcard_records_star_source() {
        use ox_query_ir::query::AggFunction;

        let ir = lineage_fixture();
        let q = match_query_with_projections(vec![Projection::Aggregation {
            function: AggFunction::Count,
            argument: None,
            alias: "row_count".into(),
            distinct: false,
        }]);
        let lineage = derive_column_lineage(&q, &ir);
        assert_eq!(lineage.len(), 1);
        let row = &lineage[0];
        assert_eq!(row.output_column, "row_count");
        // Sentinel "*" on both source_id and source_column marks
        // "scan-wide" — distinguishes a whole-row aggregation from a
        // mapping-less projection (which yields no row at all).
        assert_eq!(row.source_id, "*");
        assert_eq!(row.source_column, "*");
        assert_eq!(row.transform.as_deref(), Some("count(*)"));
    }

    #[test]
    fn expression_projection_attributes_referenced_columns() {
        use ox_query_ir::query::{ComparisonOp, Expr};

        let ir = lineage_fixture();
        // `c.name = c.tier` — references both columns. The `c.tier`
        // path carries an SqlExpr transform; the walker preserves it
        // alongside the projection's expression text via composition.
        let expr = Expr::Comparison {
            op: ComparisonOp::Eq,
            left: Box::new(Expr::Property {
                variable: vn("c"),
                field: Some(PropertyKey::new("name").unwrap()),
            }),
            right: Box::new(Expr::Property {
                variable: vn("c"),
                field: Some(PropertyKey::new("tier").unwrap()),
            }),
        };
        let q = match_query_with_projections(vec![Projection::Expression {
            expr,
            alias: "match_flag".into(),
        }]);
        let lineage = derive_column_lineage(&q, &ir);
        let cols: Vec<&str> = lineage.iter().map(|l| l.source_column.as_str()).collect();
        assert!(cols.contains(&"customers.full_name"), "{lineage:#?}");
        assert!(cols.contains(&"customers.raw_tier"), "{lineage:#?}");
        assert!(lineage.iter().all(|l| l.output_column == "match_flag"));
    }

    #[test]
    fn expression_projection_silently_skips_whole_variable_property_refs() {
        // `Expr::Property { field: None }` is a whole-variable handle
        // (e.g. `id(n)`) — there is no single source column to point
        // at (PK may be composite). Emitting a fan-out across every
        // declared property would be a heuristic lie. Honest answer:
        // emit nothing for that arm, only individual `field: Some`
        // refs land in the lineage.
        use ox_query_ir::query::Expr;

        let ir = lineage_fixture();
        let expr = Expr::Property {
            variable: vn("c"),
            field: None,
        };
        let q = match_query_with_projections(vec![Projection::Expression {
            expr,
            alias: "node_handle".into(),
        }]);
        let lineage = derive_column_lineage(&q, &ir);
        assert!(
            lineage.is_empty(),
            "field=None must not fan out across properties: {lineage:#?}",
        );
    }
}
