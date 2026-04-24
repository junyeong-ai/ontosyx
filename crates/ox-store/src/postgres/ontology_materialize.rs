//! Shared Level-3 materialisation + hydration helpers. Both
//! OntologyVersionStore (write-side, on commit) and
//! OntologyNavigationStore (read-side, on hydrate) call into
//! these to keep the flat-index tables in sync with the
//! canonical OntologyIR.

use super::*;


/// Λ-10 — Level 3 populator. Called at the end of
/// `commit_version` inside the same transaction. Fans the IR's
/// already-assembled entities into the per-kind flat indexes,
/// the `entity_neighbors` 1-hop graph, and the hierarchical
/// closure table.
///
/// The `entity_hash` column on flat rows points at the OWNER's
/// hash in Level 2 — for nested entities (Property inside
/// NodeType / EdgeType; CodedValue inside CodeSystem) the hash
/// is the parent's, since Level 2 stores the parent as the
/// single immutable unit.
///
/// Embedding rows are NOT populated here. Embedding population
/// is async (Gemini API round trip), handled by a separate
/// background task that fills `ontology_entity_embedding`
/// rows when they land.
pub(super) async fn materialize_level3(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    version_id: Uuid,
    ir: &ox_ontology::OntologyIR,
) -> OxResult<()> {
    use ox_ontology::storage::extract_entities;

    let entities = extract_entities(ir)?;
    // Build a quick `(kind, logical_id) → hash` lookup so
    // neighbour edges reference the right hash without a second
    // extract pass.
    let hash_by_id: std::collections::HashMap<
        (ox_ontology::storage::EntityKind, String),
        String,
    > = entities
        .iter()
        .map(|e| ((e.kind, e.logical_id.clone()), e.hash.clone()))
        .collect();

    // ------------------------------------------------------------
    // (A) Flat per-kind indexes
    // ------------------------------------------------------------

    // node_type
    for nt in ir.node_types() {
        let hash = hash_by_id
            .get(&(
                ox_ontology::storage::EntityKind::NodeType,
                nt.id.to_string(),
            ))
            .cloned()
            .unwrap_or_default();
        sqlx::query(
            "INSERT INTO ontology_node_type_index \
                (version_id, logical_id, entity_hash, label, deprecated_at) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(version_id)
        .bind(nt.id.as_str())
        .bind(&hash)
        .bind(nt.label.as_str())
        .bind(nt.deprecated_at)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;

        // property (nested inside node_type)
        for prop in &nt.properties {
            insert_property_row(
                tx,
                version_id,
                "node_type",
                nt.id.as_str(),
                &hash,
                prop,
            )
            .await?;
        }
    }

    // edge_type
    for et in ir.edge_types() {
        let hash = hash_by_id
            .get(&(
                ox_ontology::storage::EntityKind::EdgeType,
                et.id.to_string(),
            ))
            .cloned()
            .unwrap_or_default();
        sqlx::query(
            "INSERT INTO ontology_edge_type_index \
                (version_id, logical_id, entity_hash, label, \
                 source_type_id, target_type_id, deprecated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(version_id)
        .bind(et.id.as_str())
        .bind(&hash)
        .bind(et.label.as_str())
        .bind(et.source_node_id.as_str())
        .bind(et.target_node_id.as_str())
        .bind(et.deprecated_at)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;

        for prop in &et.properties {
            insert_property_row(
                tx,
                version_id,
                "edge_type",
                et.id.as_str(),
                &hash,
                prop,
            )
            .await?;
        }
    }

    // interface
    for iface in ir.interfaces() {
        let hash = hash_for(&hash_by_id, ox_ontology::storage::EntityKind::Interface, &iface.id);
        sqlx::query(
            "INSERT INTO ontology_interface_index \
                (version_id, logical_id, entity_hash, label) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(version_id)
        .bind(iface.id.as_str())
        .bind(&hash)
        .bind(iface.label.as_str())
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // object_mapping
    for om in ir.object_mappings() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::ObjectMapping,
            &om.id,
        );
        sqlx::query(
            "INSERT INTO ontology_object_mapping_index \
                (version_id, logical_id, entity_hash, node_type_id, \
                 source_id, precedence) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(version_id)
        .bind(om.id.as_str())
        .bind(&hash)
        .bind(om.node_type_id.as_str())
        .bind(om.source_id.as_str())
        .bind(om.precedence as i16)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // link_mapping
    for lm in ir.link_mappings() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::LinkMapping,
            &lm.id,
        );
        let kind_tag = match &lm.kind {
            ox_ontology::mapping::LinkMappingKind::ForeignKey { .. } => "foreign_key",
            ox_ontology::mapping::LinkMappingKind::Bridge { .. } => "bridge",
            ox_ontology::mapping::LinkMappingKind::Computed { .. } => "computed",
            ox_ontology::mapping::LinkMappingKind::Federated { .. } => "federated",
        };
        let cardinality = match lm.cardinality {
            ox_ontology::mapping::LinkCardinality::OneToOne => "one_to_one",
            ox_ontology::mapping::LinkCardinality::OneToMany => "one_to_many",
            ox_ontology::mapping::LinkCardinality::ManyToOne => "many_to_one",
            ox_ontology::mapping::LinkCardinality::ManyToMany => "many_to_many",
        };
        sqlx::query(
            "INSERT INTO ontology_link_mapping_index \
                (version_id, logical_id, entity_hash, edge_type_id, \
                 kind, cardinality, precedence) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(version_id)
        .bind(lm.id.as_str())
        .bind(&hash)
        .bind(lm.edge_type_id.as_str())
        .bind(kind_tag)
        .bind(cardinality)
        .bind(lm.precedence as i16)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // code_system + nested coded_value
    for cs in ir.code_systems() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::CodeSystem,
            &cs.id,
        );
        let kind_tag = match cs.kind {
            ox_ontology::code_system::CodeSystemKind::Internal => "internal",
            ox_ontology::code_system::CodeSystemKind::External { .. } => "external",
        };
        sqlx::query(
            "INSERT INTO ontology_code_system_index \
                (version_id, logical_id, entity_hash, name, uri, kind, hierarchical) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(version_id)
        .bind(cs.id.as_str())
        .bind(&hash)
        .bind(&cs.name)
        .bind(cs.uri.as_deref())
        .bind(kind_tag)
        .bind(cs.hierarchical)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;

        for cv in &cs.codes {
            sqlx::query(
                "INSERT INTO ontology_coded_value_index \
                    (version_id, logical_id, entity_hash, code_system_id, \
                     code, broader_id, deprecated_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(version_id)
            .bind(cv.id.as_str())
            .bind(&hash)
            .bind(cs.id.as_str())
            .bind(&cv.code)
            .bind(cv.broader_id.as_ref().map(|id| id.as_str()))
            .bind(cv.deprecated_at)
            .execute(&mut **tx)
            .await
            .map_err(to_ox_error)?;
        }
    }

    // value_set
    for vs in ir.value_sets() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::ValueSet,
            &vs.id,
        );
        sqlx::query(
            "INSERT INTO ontology_value_set_index \
                (version_id, logical_id, entity_hash, name) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(version_id)
        .bind(vs.id.as_str())
        .bind(&hash)
        .bind(&vs.name)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // notation_pattern
    for np in ir.notation_patterns() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::NotationPattern,
            &np.id,
        );
        sqlx::query(
            "INSERT INTO ontology_notation_pattern_index \
                (version_id, logical_id, entity_hash, name) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(version_id)
        .bind(np.id.as_str())
        .bind(&hash)
        .bind(&np.name)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // concept_map
    for cm in ir.concept_maps() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::ConceptMap,
            &cm.id,
        );
        sqlx::query(
            "INSERT INTO ontology_concept_map_index \
                (version_id, logical_id, entity_hash, name, \
                 source_system_id, target_system_id) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(version_id)
        .bind(cm.id.as_str())
        .bind(&hash)
        .bind(&cm.name)
        .bind(cm.source_system_id.as_str())
        .bind(cm.target_system_id.as_str())
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // value_range_set
    for rs in ir.value_range_sets() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::ValueRangeSet,
            &rs.id,
        );
        sqlx::query(
            "INSERT INTO ontology_value_range_set_index \
                (version_id, logical_id, entity_hash, name) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(version_id)
        .bind(rs.id.as_str())
        .bind(&hash)
        .bind(&rs.name)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // glossary_term
    for term in ir.glossary() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::GlossaryTerm,
            &term.id,
        );
        sqlx::query(
            "INSERT INTO ontology_glossary_term_index \
                (version_id, logical_id, entity_hash, term, category, parent_term_id) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(version_id)
        .bind(term.id.as_str())
        .bind(&hash)
        .bind(&term.term)
        .bind(term.category.as_deref())
        .bind(term.parent_term_id.as_ref().map(|id| id.as_str()))
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // rule
    for rule in ir.rules() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::Rule,
            &rule.id,
        );
        let kind_tag = match &rule.kind {
            ox_ontology::rule::RuleKind::NodeShape { .. } => "node_shape",
            ox_ontology::rule::RuleKind::PropertyShape { .. } => "property_shape",
            ox_ontology::rule::RuleKind::EdgeShape { .. } => "edge_shape",
            ox_ontology::rule::RuleKind::CrossEntityShape { .. } => "cross_entity_shape",
            ox_ontology::rule::RuleKind::StateMachine { .. } => "state_machine",
        };
        let severity_tag = match rule.severity {
            ox_ontology::rule::Severity::Violation => "violation",
            ox_ontology::rule::Severity::Warning => "warning",
            ox_ontology::rule::Severity::Info => "info",
        };
        sqlx::query(
            "INSERT INTO ontology_rule_index \
                (version_id, logical_id, entity_hash, kind, severity) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(version_id)
        .bind(rule.id.as_str())
        .bind(&hash)
        .bind(kind_tag)
        .bind(severity_tag)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // function
    for func in ir.functions() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::Function,
            &func.id,
        );
        let purity_tag = match func.purity {
            ox_ontology::function::FunctionPurity::Pure => "pure",
            ox_ontology::function::FunctionPurity::Impure => "impure",
        };
        sqlx::query(
            "INSERT INTO ontology_function_index \
                (version_id, logical_id, entity_hash, name, purity) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(version_id)
        .bind(func.id.as_str())
        .bind(&hash)
        .bind(&func.name)
        .bind(purity_tag)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // metric
    for metric in ir.metrics() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::Metric,
            &metric.id,
        );
        let grain_tag = match metric.temporal_grain {
            ox_ontology::metric::TemporalGrain::Snapshot => "snapshot",
            ox_ontology::metric::TemporalGrain::Daily => "daily",
            ox_ontology::metric::TemporalGrain::Weekly => "weekly",
            ox_ontology::metric::TemporalGrain::Monthly => "monthly",
            ox_ontology::metric::TemporalGrain::Quarterly => "quarterly",
            ox_ontology::metric::TemporalGrain::Yearly => "yearly",
        };
        sqlx::query(
            "INSERT INTO ontology_metric_index \
                (version_id, logical_id, entity_hash, name, temporal_grain) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(version_id)
        .bind(metric.id.as_str())
        .bind(&hash)
        .bind(&metric.name)
        .bind(grain_tag)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // ------------------------------------------------------------
    // (B) Neighbor edges — cross-references between entities.
    // ------------------------------------------------------------

    insert_neighbors_from_ir(tx, version_id, ir).await?;

    // ------------------------------------------------------------
    // (C) Hierarchical closure — code_system broader, glossary
    //     parent, interface implements.
    // ------------------------------------------------------------

    insert_hierarchy_closure(tx, version_id, ir).await?;

    // ------------------------------------------------------------
    // (D) Search vectors — flattened text + tsvector.
    // ------------------------------------------------------------

    insert_search_vectors(tx, version_id, ir).await?;

    Ok(())
}

/// Lookup helper for the `(kind, logical_id) → hash` cache built
/// at the start of `materialize_level3`. Missing entries return
/// an empty string, which then fails the FK check on the flat
/// insert — defensive: if the hash cache is out of sync with the
/// IR it is better to fail loudly here than to insert a flat row
/// pointing at nothing.
fn hash_for(
    cache: &std::collections::HashMap<(ox_ontology::storage::EntityKind, String), String>,
    kind: ox_ontology::storage::EntityKind,
    id: &impl ToString,
) -> String {
    cache
        .get(&(kind, id.to_string()))
        .cloned()
        .unwrap_or_default()
}

/// Insert one property row. Property is nested at the IR level,
/// so `owner_hash` is the NodeType / EdgeType's hash (the
/// content-addressed unit that owns this property).
#[allow(clippy::too_many_arguments)]
async fn insert_property_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    version_id: Uuid,
    owner_kind: &str,
    owner_logical_id: &str,
    owner_hash: &str,
    prop: &ox_ontology::ir::PropertyDef,
) -> OxResult<()> {
    let property_type_tag = match &prop.property_type {
        ox_core::types::PropertyType::Bool => "bool",
        ox_core::types::PropertyType::Int => "int",
        ox_core::types::PropertyType::Float => "float",
        ox_core::types::PropertyType::String => "string",
        ox_core::types::PropertyType::Date => "date",
        ox_core::types::PropertyType::DateTime => "datetime",
        ox_core::types::PropertyType::Duration => "duration",
        ox_core::types::PropertyType::Bytes => "bytes",
        ox_core::types::PropertyType::List { .. } => "list",
        ox_core::types::PropertyType::Map => "map",
    };
    let aggregation_role_tag = prop.aggregation_role.map(|r| match r {
        ox_ontology::ir::AggregationRole::Measure => "measure",
        ox_ontology::ir::AggregationRole::Dimension => "dimension",
        ox_ontology::ir::AggregationRole::Attribute => "attribute",
        ox_ontology::ir::AggregationRole::Identifier => "identifier",
    });
    let semantic_type_tag = prop.semantic_type.as_ref().map(|st| match st {
        ox_ontology::ir::SemanticType::Email => "email".to_string(),
        ox_ontology::ir::SemanticType::Phone => "phone".to_string(),
        ox_ontology::ir::SemanticType::Url => "url".to_string(),
        ox_ontology::ir::SemanticType::Address => "address".to_string(),
        ox_ontology::ir::SemanticType::Coordinate => "coordinate".to_string(),
        ox_ontology::ir::SemanticType::Currency => "currency".to_string(),
        ox_ontology::ir::SemanticType::Percentage => "percentage".to_string(),
        ox_ontology::ir::SemanticType::Iso8601 => "iso8601".to_string(),
        ox_ontology::ir::SemanticType::LocalizedText => "localized_text".to_string(),
        ox_ontology::ir::SemanticType::Other(s) => format!("other:{s}"),
    });
    let pii_kind_tag = prop.pii_kind.as_ref().map(|k| {
        // Use the enum's tag-only rendering. `serde_json::to_value`
        // on an internally-tagged enum produces {"kind": "...", ...}
        // — we pull the tag out for the flat index.
        serde_json::to_value(k)
            .ok()
            .and_then(|v| v.get("kind").and_then(|t| t.as_str()).map(String::from))
            .unwrap_or_else(|| "unknown".into())
    });

    sqlx::query(
        "INSERT INTO ontology_property_index \
            (version_id, owner_kind, owner_logical_id, logical_id, \
             entity_hash, key, property_type, nullable, is_localized, \
             aggregation_role, value_set_id, notation_pattern_id, \
             value_range_set_id, semantic_type, pii_kind, unit_id, \
             glossary_term_id, deprecated_at) \
         VALUES ($1, $2::ontology_entity_kind, $3, $4, $5, $6, $7, $8, $9, \
                 $10, $11, $12, $13, $14, $15, $16, $17, $18)",
    )
    .bind(version_id)
    .bind(owner_kind)
    .bind(owner_logical_id)
    .bind(prop.id.as_str())
    .bind(owner_hash)
    .bind(prop.name.as_str())
    .bind(property_type_tag)
    .bind(prop.nullable)
    .bind(prop.is_localized)
    .bind(aggregation_role_tag)
    .bind(prop.value_set_id.as_ref().map(|id| id.as_str()))
    .bind(prop.notation_pattern_id.as_ref().map(|id| id.as_str()))
    .bind(prop.value_range_set_id.as_ref().map(|id| id.as_str()))
    .bind(semantic_type_tag)
    .bind(pii_kind_tag)
    .bind(prop.unit_id.as_ref().map(|id| id.as_str()))
    .bind(prop.glossary_term_id.as_ref().map(|id| id.as_str()))
    .bind(prop.deprecated_at)
    .execute(&mut **tx)
    .await
    .map_err(to_ox_error)?;
    Ok(())
}

/// Harvest cross-entity references from the IR and emit 1-hop
/// neighbor edges. Kept as a free function rather than expanding
/// `materialize_level3` further so the edge-kind taxonomy is in
/// one place.
async fn insert_neighbors_from_ir(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    version_id: Uuid,
    ir: &ox_ontology::OntologyIR,
) -> OxResult<()> {
    let mut from_kinds: Vec<&str> = Vec::new();
    let mut from_ids: Vec<String> = Vec::new();
    let mut to_kinds: Vec<&str> = Vec::new();
    let mut to_ids: Vec<String> = Vec::new();
    let mut relations: Vec<&str> = Vec::new();

    let mut push = |fk: &'static str, fi: &str, tk: &'static str, ti: &str, rk: &'static str| {
        from_kinds.push(fk);
        from_ids.push(fi.to_string());
        to_kinds.push(tk);
        to_ids.push(ti.to_string());
        relations.push(rk);
    };

    // Property → value_set / notation_pattern / value_range_set /
    // glossary_term / unit (coded_value).
    let walk_properties = |props: &[ox_ontology::ir::PropertyDef], cb: &mut dyn FnMut(&ox_ontology::ir::PropertyDef)| {
        for p in props {
            cb(p);
        }
    };

    // `push` is an FnMut closure that borrows the vecs; we call
    // it from the property walk below.
    let mut on_prop = |prop: &ox_ontology::ir::PropertyDef| {
        if let Some(vs_id) = &prop.value_set_id {
            push("property", prop.id.as_str(), "value_set", vs_id.as_str(), "references_value_set");
        }
        if let Some(np_id) = &prop.notation_pattern_id {
            push("property", prop.id.as_str(), "notation_pattern", np_id.as_str(), "references_notation_pattern");
        }
        if let Some(rs_id) = &prop.value_range_set_id {
            push("property", prop.id.as_str(), "value_range_set", rs_id.as_str(), "references_value_range_set");
        }
        if let Some(gt_id) = &prop.glossary_term_id {
            push("property", prop.id.as_str(), "glossary_term", gt_id.as_str(), "references_glossary_term");
        }
        if let Some(unit_id) = &prop.unit_id {
            push("property", prop.id.as_str(), "coded_value", unit_id.as_str(), "uses_unit");
        }
        if let Some(fn_id) = &prop.derived_from {
            push("property", prop.id.as_str(), "function", fn_id.as_str(), "derived_from");
        }
    };
    for nt in ir.node_types() {
        walk_properties(&nt.properties, &mut on_prop);
    }
    for et in ir.edge_types() {
        walk_properties(&et.properties, &mut on_prop);
    }

    // ObjectMapping → NodeType
    for om in ir.object_mappings() {
        push("object_mapping", om.id.as_str(), "node_type", om.node_type_id.as_str(), "maps_node_type");
    }

    // LinkMapping → EdgeType
    for lm in ir.link_mappings() {
        push("link_mapping", lm.id.as_str(), "edge_type", lm.edge_type_id.as_str(), "maps_edge_type");
    }

    // ConceptMap → source_system / target_system
    for cm in ir.concept_maps() {
        push("concept_map", cm.id.as_str(), "code_system", cm.source_system_id.as_str(), "concept_map_source");
        push("concept_map", cm.id.as_str(), "code_system", cm.target_system_id.as_str(), "concept_map_target");
    }

    // ValueSet → CodeSystem (composition rules)
    for vs in ir.value_sets() {
        for rule in &vs.composition {
            push(
                "value_set",
                vs.id.as_str(),
                "code_system",
                rule.system_id.as_str(),
                "value_set_includes_system",
            );
        }
    }

    if from_kinds.is_empty() {
        return Ok(());
    }

    let from_kinds_owned: Vec<String> = from_kinds.iter().map(|s| s.to_string()).collect();
    let to_kinds_owned: Vec<String> = to_kinds.iter().map(|s| s.to_string()).collect();
    let relations_owned: Vec<String> = relations.iter().map(|s| s.to_string()).collect();

    sqlx::query(
        "INSERT INTO ontology_entity_neighbors \
            (version_id, from_kind, from_logical_id, to_kind, to_logical_id, relation_kind) \
         SELECT $1, fk.fkind::ontology_entity_kind, fk.fid, \
                    fk.tkind::ontology_entity_kind, fk.tid, fk.rk \
         FROM UNNEST($2::text[], $3::text[], $4::text[], $5::text[], $6::text[]) \
              AS fk(fkind, fid, tkind, tid, rk) \
         ON CONFLICT DO NOTHING",
    )
    .bind(version_id)
    .bind(&from_kinds_owned)
    .bind(&from_ids)
    .bind(&to_kinds_owned)
    .bind(&to_ids)
    .bind(&relations_owned)
    .execute(&mut **tx)
    .await
    .map_err(to_ox_error)?;

    Ok(())
}

/// Materialise the hierarchical closure. Three relations today:
///
///   code_system_broader      CodedValue.broader_id inside a
///                            hierarchical CodeSystem.
///   glossary_term_parent     GlossaryTermDef.parent_term_id.
///   interface_implements     NodeType.implements → Interface.
///
/// Closure is built in-memory via iterative fixpoint. Input
/// sizes are small (low thousands), so the O(n²) worst case is
/// fine; a future enterprise-scale growth would migrate this
/// to a recursive CTE stored proc.
async fn insert_hierarchy_closure(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    version_id: Uuid,
    ir: &ox_ontology::OntologyIR,
) -> OxResult<()> {
    // Each row: (relation_kind, ancestor_kind, ancestor_id,
    // descendant_kind, descendant_id, depth).
    let mut rows: Vec<(String, String, String, String, String, i32)> = Vec::new();

    // 1) code_system_broader — walk CodedValue.broader_id per system.
    for cs in ir.code_systems() {
        if !cs.hierarchical {
            continue;
        }
        // Build immediate parent map.
        let parent_of: std::collections::HashMap<&str, &str> = cs
            .codes
            .iter()
            .filter_map(|cv| cv.broader_id.as_ref().map(|b| (cv.id.as_str(), b.as_str())))
            .collect();
        for cv in &cs.codes {
            // self — depth 0
            rows.push((
                "code_system_broader".into(),
                "coded_value".into(),
                cv.id.to_string(),
                "coded_value".into(),
                cv.id.to_string(),
                0,
            ));
            // Walk ancestors.
            let mut current = cv.id.as_str();
            let mut depth = 1;
            let limit = cs.codes.len() + 1;
            let mut guard = 0;
            while let Some(parent) = parent_of.get(current) {
                rows.push((
                    "code_system_broader".into(),
                    "coded_value".into(),
                    parent.to_string(),
                    "coded_value".into(),
                    cv.id.to_string(),
                    depth,
                ));
                current = parent;
                depth += 1;
                guard += 1;
                if guard >= limit {
                    break; // cycle guard
                }
            }
        }
    }

    // 2) glossary_term_parent — walk GlossaryTermDef.parent_term_id.
    let terms: Vec<_> = ir.glossary().iter().collect();
    let parent_map: std::collections::HashMap<&str, &str> = terms
        .iter()
        .filter_map(|t| t.parent_term_id.as_ref().map(|p| (t.id.as_str(), p.as_str())))
        .collect();
    for term in &terms {
        rows.push((
            "glossary_term_parent".into(),
            "glossary_term".into(),
            term.id.to_string(),
            "glossary_term".into(),
            term.id.to_string(),
            0,
        ));
        let mut current = term.id.as_str();
        let mut depth = 1;
        let limit = terms.len() + 1;
        let mut guard = 0;
        while let Some(parent) = parent_map.get(current) {
            rows.push((
                "glossary_term_parent".into(),
                "glossary_term".into(),
                parent.to_string(),
                "glossary_term".into(),
                term.id.to_string(),
                depth,
            ));
            current = parent;
            depth += 1;
            guard += 1;
            if guard >= limit {
                break;
            }
        }
    }

    // 3) interface_implements — NodeType → Interface for each of
    //    the node's `implements` entries. NodeTypeDef's
    //    `implements` field holds `Vec<InterfaceId>`.
    for nt in ir.node_types() {
        for iface_id in &nt.implements {
            rows.push((
                "interface_implements".into(),
                "node_type".into(),
                nt.id.to_string(),
                "interface".into(),
                iface_id.to_string(),
                1,
            ));
        }
    }

    if rows.is_empty() {
        return Ok(());
    }

    // Bulk insert via UNNEST of six parallel arrays.
    let mut rel: Vec<String> = Vec::with_capacity(rows.len());
    let mut ak: Vec<String> = Vec::with_capacity(rows.len());
    let mut ai: Vec<String> = Vec::with_capacity(rows.len());
    let mut dk: Vec<String> = Vec::with_capacity(rows.len());
    let mut di: Vec<String> = Vec::with_capacity(rows.len());
    let mut dp: Vec<i32> = Vec::with_capacity(rows.len());
    for r in rows {
        rel.push(r.0);
        ak.push(r.1);
        ai.push(r.2);
        dk.push(r.3);
        di.push(r.4);
        dp.push(r.5);
    }
    sqlx::query(
        "INSERT INTO ontology_entity_hierarchy \
            (version_id, relation_kind, ancestor_kind, ancestor_logical_id, \
             descendant_kind, descendant_logical_id, depth) \
         SELECT $1, \
                r.rel, \
                r.ak::ontology_entity_kind, r.ai, \
                r.dk::ontology_entity_kind, r.di, \
                r.dp \
         FROM UNNEST($2::text[], $3::text[], $4::text[], $5::text[], $6::text[], $7::int[]) \
              AS r(rel, ak, ai, dk, di, dp) \
         ON CONFLICT DO NOTHING",
    )
    .bind(version_id)
    .bind(&rel)
    .bind(&ak)
    .bind(&ai)
    .bind(&dk)
    .bind(&di)
    .bind(&dp)
    .execute(&mut **tx)
    .await
    .map_err(to_ox_error)?;

    Ok(())
}

/// Build the `ontology_entity_search_vector` row per entity.
/// `doc` is the concatenated searchable text; `tsv` is
/// `to_tsvector('simple', doc)`. `simple` dictionary preserves
/// exact tokens across the mixed-language content our customers
/// author.
async fn insert_search_vectors(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    version_id: Uuid,
    ir: &ox_ontology::OntologyIR,
) -> OxResult<()> {
    // Per-entity docs. The ontology_header row covers the
    // ontology-level searchable text (name + description).
    let mut kinds: Vec<String> = Vec::new();
    let mut lids: Vec<String> = Vec::new();
    let mut docs: Vec<String> = Vec::new();

    let mut emit = |kind: &'static str, lid: &str, doc: String| {
        kinds.push(kind.to_string());
        lids.push(lid.to_string());
        docs.push(doc);
    };

    let localized_flat = |t: &ox_core::i18n::LocalizedText| {
        // Default + every translation joined with spaces. Skips
        // empty strings so the docvector doesn't inflate with
        // whitespace.
        let mut parts = Vec::new();
        if !t.default.is_empty() {
            parts.push(t.default.clone());
        }
        for v in t.translations.values() {
            if !v.is_empty() {
                parts.push(v.clone());
            }
        }
        parts.join(" ")
    };

    emit(
        "ontology_header",
        &ir.id,
        format!("{} {}", ir.name, localized_flat(&ir.description)),
    );

    for nt in ir.node_types() {
        emit(
            "node_type",
            nt.id.as_str(),
            format!(
                "{} {}",
                nt.label.as_str(),
                localized_flat(&nt.description)
            ),
        );
        for prop in &nt.properties {
            let aliases = prop
                .aliases
                .iter()
                .map(localized_flat)
                .collect::<Vec<_>>()
                .join(" ");
            emit(
                "property",
                prop.id.as_str(),
                format!(
                    "{} {} {} {}",
                    prop.name.as_str(),
                    localized_flat(&prop.display_name),
                    aliases,
                    localized_flat(&prop.description)
                ),
            );
        }
    }
    for et in ir.edge_types() {
        emit(
            "edge_type",
            et.id.as_str(),
            format!(
                "{} {}",
                et.label.as_str(),
                localized_flat(&et.description)
            ),
        );
    }
    for cs in ir.code_systems() {
        emit(
            "code_system",
            cs.id.as_str(),
            format!(
                "{} {} {}",
                cs.name,
                localized_flat(&cs.display_name),
                localized_flat(&cs.description)
            ),
        );
        for cv in &cs.codes {
            let alias = cv.aliases.join(" ");
            emit(
                "coded_value",
                cv.id.as_str(),
                format!(
                    "{} {} {} {} {}",
                    cv.code,
                    localized_flat(&cv.display),
                    localized_flat(&cv.definition),
                    alias,
                    localized_flat(&cv.scope_note)
                ),
            );
        }
    }
    for vs in ir.value_sets() {
        emit(
            "value_set",
            vs.id.as_str(),
            format!(
                "{} {} {}",
                vs.name,
                localized_flat(&vs.display_name),
                localized_flat(&vs.description)
            ),
        );
    }
    for np in ir.notation_patterns() {
        emit(
            "notation_pattern",
            np.id.as_str(),
            format!(
                "{} {} {}",
                np.name,
                localized_flat(&np.display_name),
                localized_flat(&np.description)
            ),
        );
    }
    for term in ir.glossary() {
        let aliases = term.aliases.join(" ");
        emit(
            "glossary_term",
            term.id.as_str(),
            format!(
                "{} {} {} {}",
                term.term,
                localized_flat(&term.display_name),
                aliases,
                localized_flat(&term.description)
            ),
        );
    }

    if kinds.is_empty() {
        return Ok(());
    }

    sqlx::query(
        "INSERT INTO ontology_entity_search_vector \
            (version_id, entity_kind, logical_id, doc, tsv) \
         SELECT $1, k::ontology_entity_kind, l, d, to_tsvector('simple', d) \
         FROM UNNEST($2::text[], $3::text[], $4::text[]) AS s(k, l, d)",
    )
    .bind(version_id)
    .bind(&kinds)
    .bind(&lids)
    .bind(&docs)
    .execute(&mut **tx)
    .await
    .map_err(to_ox_error)?;

    Ok(())
}

/// Assemble an `OntologyIR` from a flat list of
/// `(kind, logical_id, hash, content)` rows. Groups rows by kind
/// and routes each group into the matching IR collection. The
/// header row produces the outer IR struct.
///
/// Returns `OxResult` so a malformed stored row (kind that
/// doesn't parse, content that doesn't deserialise, missing
/// header) surfaces with a specific error — downstream callers
/// map to a 500 rather than silently filling a half-empty IR.
pub(super) fn assemble_ir(
    rows: &[crate::models::OntologyEntityJoinRow],
) -> OxResult<ox_ontology::OntologyIR> {
    use ox_ontology::storage::EntityKind;

    let mut header: Option<serde_json::Value> = None;
    let mut node_types: Vec<ox_ontology::ir::NodeTypeDef> = Vec::new();
    let mut edge_types: Vec<ox_ontology::ir::EdgeTypeDef> = Vec::new();
    let mut indexes: Vec<ox_ontology::ir::IndexDef> = Vec::new();
    let mut interfaces: Vec<ox_ontology::interface::InterfaceDef> = Vec::new();
    let mut object_mappings: Vec<ox_ontology::mapping::ObjectMappingDef> = Vec::new();
    let mut link_mappings: Vec<ox_ontology::mapping::LinkMappingDef> = Vec::new();
    let mut rules: Vec<ox_ontology::rule::RuleDef> = Vec::new();
    let mut data_quality: Vec<ox_ontology::data_quality::DataQualityDef> = Vec::new();
    let mut actions: Vec<ox_ontology::action::ActionDef> = Vec::new();
    let mut provenance: Vec<ox_ontology::provenance::ProvenanceDef> = Vec::new();
    let mut functions: Vec<ox_ontology::function::FunctionDef> = Vec::new();
    let mut metrics: Vec<ox_ontology::metric::MetricDef> = Vec::new();
    let mut enrichments: Vec<ox_ontology::enrichment::EnrichmentDef> = Vec::new();
    let mut glossary: Vec<ox_ontology::glossary::GlossaryTermDef> = Vec::new();
    let mut code_systems: Vec<ox_ontology::code_system::CodeSystemDef> = Vec::new();
    let mut value_sets: Vec<ox_ontology::value_set::ValueSetDef> = Vec::new();
    let mut notation_patterns: Vec<ox_ontology::notation_pattern::NotationPatternDef> =
        Vec::new();
    let mut concept_maps: Vec<ox_ontology::concept_map::ConceptMapDef> = Vec::new();
    let mut value_range_sets: Vec<ox_ontology::value_range::ValueRangeSetDef> = Vec::new();

    for row in rows {
        let kind = EntityKind::parse(&row.entity_kind)?;
        match kind {
            EntityKind::OntologyHeader => {
                header = Some(row.content.clone());
            }
            EntityKind::NodeType => node_types.push(serde_json::from_value(row.content.clone())?),
            EntityKind::EdgeType => edge_types.push(serde_json::from_value(row.content.clone())?),
            EntityKind::IndexDef => indexes.push(serde_json::from_value(row.content.clone())?),
            EntityKind::Interface => interfaces.push(serde_json::from_value(row.content.clone())?),
            EntityKind::ObjectMapping => {
                object_mappings.push(serde_json::from_value(row.content.clone())?)
            }
            EntityKind::LinkMapping => {
                link_mappings.push(serde_json::from_value(row.content.clone())?)
            }
            EntityKind::PropertyMapping => {
                // PropertyMappingDef is nested inside ObjectMappingDef in
                // the current IR — it rides along with its parent. When
                // the IR model promotes it to a top-level collection,
                // this arm routes into the new vector.
            }
            EntityKind::Rule => rules.push(serde_json::from_value(row.content.clone())?),
            EntityKind::DataQuality => {
                data_quality.push(serde_json::from_value(row.content.clone())?)
            }
            EntityKind::Action => actions.push(serde_json::from_value(row.content.clone())?),
            EntityKind::Provenance => {
                provenance.push(serde_json::from_value(row.content.clone())?)
            }
            EntityKind::Function => functions.push(serde_json::from_value(row.content.clone())?),
            EntityKind::Metric => metrics.push(serde_json::from_value(row.content.clone())?),
            EntityKind::Enrichment => {
                enrichments.push(serde_json::from_value(row.content.clone())?)
            }
            EntityKind::GlossaryTerm => {
                glossary.push(serde_json::from_value(row.content.clone())?)
            }
            EntityKind::Taxonomy => {
                // Same deferral as PropertyMapping — not yet an
                // independent IR collection. Lands when the IR model
                // promotes Taxonomy out of the glossary module.
            }
            EntityKind::CodeSystem => {
                code_systems.push(serde_json::from_value(row.content.clone())?)
            }
            EntityKind::ValueSet => value_sets.push(serde_json::from_value(row.content.clone())?),
            EntityKind::NotationPattern => {
                notation_patterns.push(serde_json::from_value(row.content.clone())?)
            }
            EntityKind::ConceptMap => {
                concept_maps.push(serde_json::from_value(row.content.clone())?)
            }
            EntityKind::ValueRangeSet => {
                value_range_sets.push(serde_json::from_value(row.content.clone())?)
            }
        }
    }

    // Header parse — must be present exactly once. Deserialising it
    // gives the outer-struct scalars (id, name, description, version,
    // schema_version).
    let header = header.ok_or_else(|| OxError::Runtime {
        message: "version pointer set is missing the ontology_header entity".into(),
    })?;
    #[derive(serde::Deserialize)]
    struct HeaderWire {
        id: String,
        name: String,
        #[serde(default)]
        description: ox_core::i18n::LocalizedText,
        version: ox_ontology::ir::OntologyVersion,
        #[serde(default)]
        schema_version: u32,
    }
    let h: HeaderWire = serde_json::from_value(header)?;
    let _ = h.schema_version; // the current build's version is authoritative

    let mut ir = ox_ontology::OntologyIR::try_new(
        h.id,
        h.name,
        h.description,
        h.version,
        node_types,
        edge_types,
        indexes,
    )
    .map_err(|e| OxError::Runtime {
        message: format!("OntologyIR::try_new rejected rebuilt topology: {e:?}"),
    })?;

    for iface in interfaces {
        ir.add_interface(iface).map_err(|e| OxError::Runtime {
            message: format!("add_interface during hydration: {e:?}"),
        })?;
    }
    for om in object_mappings {
        ir.add_object_mapping(om).map_err(|e| OxError::Runtime {
            message: format!("add_object_mapping during hydration: {e:?}"),
        })?;
    }
    for lm in link_mappings {
        ir.add_link_mapping(lm).map_err(|e| OxError::Runtime {
            message: format!("add_link_mapping during hydration: {e:?}"),
        })?;
    }
    for rule in rules {
        ir.add_rule(rule).map_err(|e| OxError::Runtime {
            message: format!("add_rule during hydration: {e:?}"),
        })?;
    }
    for dq in data_quality {
        ir.add_data_quality(dq).map_err(|e| OxError::Runtime {
            message: format!("add_data_quality during hydration: {e:?}"),
        })?;
    }
    for action in actions {
        ir.add_action(action).map_err(|e| OxError::Runtime {
            message: format!("add_action during hydration: {e:?}"),
        })?;
    }
    for prov in provenance {
        ir.add_provenance(prov);
    }
    for f in functions {
        ir.add_function(f).map_err(|e| OxError::Runtime {
            message: format!("add_function during hydration: {e:?}"),
        })?;
    }
    for m in metrics {
        ir.add_metric(m).map_err(|e| OxError::Runtime {
            message: format!("add_metric during hydration: {e:?}"),
        })?;
    }
    for e in enrichments {
        ir.add_enrichment(e).map_err(|err| OxError::Runtime {
            message: format!("add_enrichment during hydration: {err:?}"),
        })?;
    }
    for term in glossary {
        ir.add_glossary_term(term).map_err(|e| OxError::Runtime {
            message: format!("add_glossary_term during hydration: {e:?}"),
        })?;
    }
    for cs in code_systems {
        ir.add_code_system(cs).map_err(|e| OxError::Runtime {
            message: format!("add_code_system during hydration: {e:?}"),
        })?;
    }
    for vs in value_sets {
        ir.add_value_set(vs).map_err(|e| OxError::Runtime {
            message: format!("add_value_set during hydration: {e:?}"),
        })?;
    }
    for np in notation_patterns {
        ir.add_notation_pattern(np).map_err(|e| OxError::Runtime {
            message: format!("add_notation_pattern during hydration: {e:?}"),
        })?;
    }
    for cm in concept_maps {
        ir.add_concept_map(cm).map_err(|e| OxError::Runtime {
            message: format!("add_concept_map during hydration: {e:?}"),
        })?;
    }
    for rs in value_range_sets {
        ir.add_value_range_set(rs).map_err(|e| OxError::Runtime {
            message: format!("add_value_range_set during hydration: {e:?}"),
        })?;
    }

    Ok(ir)
}
