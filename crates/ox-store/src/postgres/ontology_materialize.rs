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
    let hash_by_id: std::collections::HashMap<(ox_ontology::storage::EntityKind, String), String> =
        entities
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
            insert_property_row(tx, version_id, "node_type", nt.id.as_str(), &hash, prop).await?;
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
            insert_property_row(tx, version_id, "edge_type", et.id.as_str(), &hash, prop).await?;
        }
    }

    // interface
    for iface in ir.interfaces() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::Interface,
            &iface.id,
        );
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

    // concept
    for concept in ir.concepts() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::Concept,
            &concept.id,
        );
        let alias_term_ids: Vec<&str> = concept
            .alias_term_ids
            .iter()
            .map(|id| id.as_str())
            .collect();
        sqlx::query(
            "INSERT INTO ontology_concept_index \
                (version_id, logical_id, entity_hash, canonical_term_id, \
                 alias_term_ids, broader_id, replaced_by_id, lifecycle, \
                 category, valid_from, valid_to) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(version_id)
        .bind(concept.id.as_str())
        .bind(&hash)
        .bind(concept.canonical_term_id.as_str())
        .bind(&alias_term_ids)
        .bind(concept.broader.as_ref().map(|id| id.as_str()))
        .bind(concept.replaced_by.as_ref().map(|id| id.as_str()))
        .bind(lifecycle_tag(&concept.lifecycle))
        .bind(concept.category.as_deref())
        .bind(concept.valid_from)
        .bind(concept.valid_to)
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
        let related_json = serde_json::to_value(&term.related_terms).map_err(|e| {
            ox_core::error::OxError::Runtime {
                message: format!("serialise related_terms for glossary {}: {e}", term.id),
            }
        })?;
        sqlx::query(
            "INSERT INTO ontology_glossary_term_index \
                (version_id, logical_id, entity_hash, term, category, related_terms) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(version_id)
        .bind(term.id.as_str())
        .bind(&hash)
        .bind(term.term.default.as_str())
        .bind(term.category.as_deref())
        .bind(&related_json)
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
             aggregation_role, semantic_type, pii_kind, unit_id, \
             deprecated_at) \
         VALUES ($1, $2::ontology_entity_kind, $3, $4, $5, $6, $7, $8, $9, \
                 $10, $11, $12, $13, $14)",
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
    .bind(semantic_type_tag)
    .bind(pii_kind_tag)
    .bind(prop.unit_id.as_ref().map(|id| id.as_str()))
    .bind(prop.deprecated_at)
    .execute(&mut **tx)
    .await
    .map_err(to_ox_error)?;

    // Per-binding rows. Multi-binding properties produce multiple
    // rows; single-binding (the common case) produces one. Strength
    // and target kind serialise as snake_case for index-friendly
    // string comparison on the SQL side.
    for (ordinal, binding) in prop.bindings.iter().enumerate() {
        let (target_kind, target_id) = binding_target_columns(binding);
        let strength = binding_strength_str(binding.strength());
        let (valid_from, valid_to) = binding.window();
        sqlx::query(
            "INSERT INTO ontology_property_binding \
                (version_id, owner_kind, owner_logical_id, \
                 property_logical_id, ordinal, target_kind, target_id, \
                 strength, concept_map_id, valid_from, valid_to) \
             VALUES ($1, $2::ontology_entity_kind, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(version_id)
        .bind(owner_kind)
        .bind(owner_logical_id)
        .bind(prop.id.as_str())
        .bind(ordinal as i32)
        .bind(target_kind)
        .bind(target_id)
        .bind(strength)
        .bind(binding.concept_map_id().map(|id| id.as_str()))
        .bind(valid_from)
        .bind(valid_to)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }
    Ok(())
}

/// Lower a [`PropertyBinding`] variant to the `(target_kind, target_id)`
/// SQL pair the binding table stores. `target_kind` is snake_case so
/// a `WHERE target_kind = 'value_set'` lookup in the admin UI is
/// index-friendly.
fn binding_target_columns(binding: &ox_ontology::PropertyBinding) -> (&'static str, &str) {
    use ox_ontology::PropertyBinding;
    match binding {
        PropertyBinding::ValueSet { id, .. } => ("value_set", id.as_str()),
        PropertyBinding::CodeSystem { id, .. } => ("code_system", id.as_str()),
        PropertyBinding::NotationPattern { id, .. } => ("notation_pattern", id.as_str()),
        PropertyBinding::ValueRange { id, .. } => ("value_range", id.as_str()),
        PropertyBinding::Concept { id, .. } => ("concept", id.as_str()),
    }
}

/// Lower [`BindingStrength`] to its snake_case wire token. Stored as
/// TEXT (not an enum) so adding a new strength variant is a code-only
/// change rather than a `CREATE TYPE … ADD VALUE` DDL pass.
fn binding_strength_str(s: ox_ontology::BindingStrength) -> &'static str {
    use ox_ontology::BindingStrength;
    match s {
        BindingStrength::Required => "required",
        BindingStrength::Preferred => "preferred",
        BindingStrength::Extensible => "extensible",
        BindingStrength::Example => "example",
    }
}

fn lifecycle_tag(lifecycle: &ox_ontology::glossary::TermLifecycle) -> &'static str {
    match lifecycle {
        ox_ontology::glossary::TermLifecycle::Active => "active",
        ox_ontology::glossary::TermLifecycle::Deprecated { .. } => "deprecated",
        ox_ontology::glossary::TermLifecycle::Retired { .. } => "retired",
    }
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
    // concept / unit (coded_value).
    let walk_properties =
        |props: &[ox_ontology::ir::PropertyDef],
         cb: &mut dyn FnMut(&ox_ontology::ir::PropertyDef)| {
            for p in props {
                cb(p);
            }
        };

    // `push` is an FnMut closure that borrows the vecs; we call
    // it from the property walk below. One neighbour edge per
    // binding entry — multi-binding properties surface every target
    // so the cross-axis dashboard sees the whole semantic surface.
    //
    // Every `kind` argument is sourced from `EntityKind::*::as_str()`
    // (or a `code_system::CodedValueId`-backed `code_system` parent
    // for unit refs) so the SQL `::ontology_entity_kind` cast can
    // never see a string the enum doesn't list. Adding a new
    // PropertyBinding variant forces the match below to grow,
    // surfacing the required enum addition at compile time rather
    // than as a 22P02 runtime error in production.
    let mut on_prop = |prop: &ox_ontology::ir::PropertyDef| {
        use ox_ontology::PropertyBinding;
        use ox_ontology::storage::EntityKind;
        let property_kind = EntityKind::Property.as_str();

        for binding in &prop.bindings {
            let (target_kind, target_id, relation) = match binding {
                PropertyBinding::ValueSet { id, .. } => (
                    EntityKind::ValueSet.as_str(),
                    id.as_str(),
                    "references_value_set",
                ),
                PropertyBinding::CodeSystem { id, .. } => (
                    EntityKind::CodeSystem.as_str(),
                    id.as_str(),
                    "references_code_system",
                ),
                PropertyBinding::NotationPattern { id, .. } => (
                    EntityKind::NotationPattern.as_str(),
                    id.as_str(),
                    "references_notation_pattern",
                ),
                PropertyBinding::ValueRange { id, .. } => (
                    EntityKind::ValueRangeSet.as_str(),
                    id.as_str(),
                    "references_value_range_set",
                ),
                PropertyBinding::Concept { id, .. } => (
                    EntityKind::Concept.as_str(),
                    id.as_str(),
                    "references_concept",
                ),
            };
            push(
                property_kind,
                prop.id.as_str(),
                target_kind,
                target_id,
                relation,
            );
        }
        // Units are individual `CodedValue` rows nested under a
        // `code_system`. The enum carries `coded_value` as a
        // first-class kind so the neighbour edge lands at the exact
        // unit row, not the parent system.
        if let Some(unit_id) = &prop.unit_id {
            push(
                property_kind,
                prop.id.as_str(),
                EntityKind::CodedValue.as_str(),
                unit_id.as_str(),
                "uses_unit",
            );
        }
        if let Some(fn_id) = &prop.derived_from {
            push(
                property_kind,
                prop.id.as_str(),
                EntityKind::Function.as_str(),
                fn_id.as_str(),
                "derived_from",
            );
        }
    };
    for nt in ir.node_types() {
        walk_properties(&nt.properties, &mut on_prop);
    }
    for et in ir.edge_types() {
        walk_properties(&et.properties, &mut on_prop);
    }

    use ox_ontology::storage::EntityKind;
    let object_mapping_kind = EntityKind::ObjectMapping.as_str();
    let link_mapping_kind = EntityKind::LinkMapping.as_str();
    let node_type_kind = EntityKind::NodeType.as_str();
    let edge_type_kind = EntityKind::EdgeType.as_str();
    let concept_map_kind = EntityKind::ConceptMap.as_str();
    let code_system_kind = EntityKind::CodeSystem.as_str();
    let concept_kind = EntityKind::Concept.as_str();
    let glossary_term_kind = EntityKind::GlossaryTerm.as_str();

    // NodeType / EdgeType → Concept
    for nt in ir.node_types() {
        if let Some(concept_id) = &nt.concept_id {
            push(
                node_type_kind,
                nt.id.as_str(),
                concept_kind,
                concept_id.as_str(),
                "realises_concept",
            );
        }
        for realization in &nt.concept_realizations {
            push(
                node_type_kind,
                nt.id.as_str(),
                concept_kind,
                realization.concept_id.as_str(),
                "realises_concept",
            );
        }
    }
    for et in ir.edge_types() {
        if let Some(concept_id) = &et.concept_id {
            push(
                edge_type_kind,
                et.id.as_str(),
                concept_kind,
                concept_id.as_str(),
                "realises_concept",
            );
        }
        for realization in &et.concept_realizations {
            push(
                edge_type_kind,
                et.id.as_str(),
                concept_kind,
                realization.concept_id.as_str(),
                "realises_concept",
            );
        }
    }

    // Concept → GlossaryTerm lexicalizations
    for concept in ir.concepts() {
        push(
            concept_kind,
            concept.id.as_str(),
            glossary_term_kind,
            concept.canonical_term_id.as_str(),
            "canonical_term",
        );
        for alias_id in &concept.alias_term_ids {
            push(
                concept_kind,
                concept.id.as_str(),
                glossary_term_kind,
                alias_id.as_str(),
                "alias_term",
            );
        }
    }

    // ObjectMapping → NodeType
    for om in ir.object_mappings() {
        push(
            object_mapping_kind,
            om.id.as_str(),
            node_type_kind,
            om.node_type_id.as_str(),
            "maps_node_type",
        );
    }

    // LinkMapping → EdgeType
    for lm in ir.link_mappings() {
        push(
            link_mapping_kind,
            lm.id.as_str(),
            edge_type_kind,
            lm.edge_type_id.as_str(),
            "maps_edge_type",
        );
    }

    // ConceptMap → source_system / target_system
    for cm in ir.concept_maps() {
        push(
            concept_map_kind,
            cm.id.as_str(),
            code_system_kind,
            cm.source_system_id.as_str(),
            "concept_map_source",
        );
        push(
            concept_map_kind,
            cm.id.as_str(),
            code_system_kind,
            cm.target_system_id.as_str(),
            "concept_map_target",
        );
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

    // idempotent: the neighbor row is fully determined by
    // (version_id, from_kind, from_logical_id, to_kind, to_logical_id,
    // relation_kind) — the same edge re-materialised carries the
    // same content. Re-runs of the materialisation pass are safe.
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

/// Materialise the hierarchical closure. Four relations today:
///
///   code_system_broader      CodedValue.broader_id inside a
///                            hierarchical CodeSystem.
///   concept_broader          ConceptDef.broader.
///   glossary_term_broader    GlossaryTermDef.related_terms[Broader].
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
                ox_ontology::storage::EntityKind::CodedValue.as_str().into(),
                cv.id.to_string(),
                ox_ontology::storage::EntityKind::CodedValue.as_str().into(),
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
                    ox_ontology::storage::EntityKind::CodedValue.as_str().into(),
                    parent.to_string(),
                    ox_ontology::storage::EntityKind::CodedValue.as_str().into(),
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

    // 2) concept_broader — walk ConceptDef.broader.
    let concept_parent_map: std::collections::HashMap<&str, &str> = ir
        .concepts()
        .iter()
        .filter_map(|concept| {
            concept
                .broader
                .as_ref()
                .map(|parent| (concept.id.as_str(), parent.as_str()))
        })
        .collect();
    for concept in ir.concepts() {
        rows.push((
            "concept_broader".into(),
            ox_ontology::storage::EntityKind::Concept.as_str().into(),
            concept.id.to_string(),
            ox_ontology::storage::EntityKind::Concept.as_str().into(),
            concept.id.to_string(),
            0,
        ));
        let mut current = concept.id.as_str();
        let mut depth = 1;
        let limit = ir.concepts().len() + 1;
        let mut guard = 0;
        while let Some(parent) = concept_parent_map.get(current) {
            rows.push((
                "concept_broader".into(),
                ox_ontology::storage::EntityKind::Concept.as_str().into(),
                parent.to_string(),
                ox_ontology::storage::EntityKind::Concept.as_str().into(),
                concept.id.to_string(),
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

    // 3) glossary_term_broader — walk GlossaryTermDef.related_terms
    //    for `Broader` edges (the SKOS hierarchy axis).
    let terms: Vec<_> = ir.glossary().iter().collect();
    let parent_map: std::collections::HashMap<&str, &str> = terms
        .iter()
        .filter_map(|t| {
            t.related_terms
                .iter()
                .find(|r| r.kind == ox_ontology::TermRelationKind::Broader)
                .map(|r| (t.id.as_str(), r.target.as_str()))
        })
        .collect();
    for term in &terms {
        rows.push((
            "glossary_term_broader".into(),
            ox_ontology::storage::EntityKind::GlossaryTerm
                .as_str()
                .into(),
            term.id.to_string(),
            ox_ontology::storage::EntityKind::GlossaryTerm
                .as_str()
                .into(),
            term.id.to_string(),
            0,
        ));
        let mut current = term.id.as_str();
        let mut depth = 1;
        let limit = terms.len() + 1;
        let mut guard = 0;
        while let Some(parent) = parent_map.get(current) {
            rows.push((
                "glossary_term_broader".into(),
                ox_ontology::storage::EntityKind::GlossaryTerm
                    .as_str()
                    .into(),
                parent.to_string(),
                ox_ontology::storage::EntityKind::GlossaryTerm
                    .as_str()
                    .into(),
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

    // 4) interface_implements — NodeType → Interface for each of
    //    the node's `implements` entries. NodeTypeDef's
    //    `implements` field holds `Vec<InterfaceId>`.
    for nt in ir.node_types() {
        for iface_id in &nt.implements {
            rows.push((
                "interface_implements".into(),
                ox_ontology::storage::EntityKind::NodeType.as_str().into(),
                nt.id.to_string(),
                ox_ontology::storage::EntityKind::Interface.as_str().into(),
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
    // idempotent: the hierarchy row is fully determined by
    // (version_id, relation_kind, ancestor, descendant, depth). Same
    // closure walk re-emitting the same row produces identical
    // bytes. Re-runs of the hierarchy materialisation are safe.
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
    //
    // `kind` is typed as `EntityKind` rather than a `&str` literal
    // so the SQL `::ontology_entity_kind` cast can never see a
    // string the enum doesn't list — adding a new search-indexable
    // entity forces an enum addition at compile time.
    use ox_ontology::storage::EntityKind;
    let mut kinds: Vec<String> = Vec::new();
    let mut lids: Vec<String> = Vec::new();
    let mut labels: Vec<String> = Vec::new();
    let mut docs: Vec<String> = Vec::new();

    // The label column is the *canonical* short name — what the
    // operator types when they want this entity. Distinct from
    // `doc` which carries label + aliases + description for
    // recall. Stored in its own column so the retrieval blend can
    // weight label-match independently of description-match.
    let mut emit = |kind: EntityKind, lid: &str, label: String, doc: String| {
        kinds.push(kind.as_str().to_string());
        lids.push(lid.to_string());
        labels.push(label);
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
        EntityKind::OntologyHeader,
        &ir.id,
        ir.name.clone(),
        format!("{} {}", ir.name, localized_flat(&ir.description)),
    );

    for nt in ir.node_types() {
        emit(
            EntityKind::NodeType,
            nt.id.as_str(),
            nt.label.as_str().to_string(),
            format!("{} {}", nt.label.as_str(), localized_flat(&nt.description)),
        );
        for prop in &nt.properties {
            let aliases = prop
                .aliases
                .iter()
                .map(localized_flat)
                .collect::<Vec<_>>()
                .join(" ");
            emit(
                EntityKind::Property,
                prop.id.as_str(),
                prop.name.as_str().to_string(),
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
            EntityKind::EdgeType,
            et.id.as_str(),
            et.label.as_str().to_string(),
            format!("{} {}", et.label.as_str(), localized_flat(&et.description)),
        );
    }
    for concept in ir.concepts() {
        let label = ir
            .glossary()
            .iter()
            .find(|term| term.id == concept.canonical_term_id)
            .map(|term| localized_flat(&term.term))
            .unwrap_or_else(|| concept.id.as_str().to_string());
        let aliases = concept
            .alias_term_ids
            .iter()
            .filter_map(|alias_id| {
                ir.glossary()
                    .iter()
                    .find(|term| term.id == *alias_id)
                    .map(|term| localized_flat(&term.term))
            })
            .collect::<Vec<_>>()
            .join(" ");
        emit(
            EntityKind::Concept,
            concept.id.as_str(),
            label.clone(),
            format!(
                "{} {} {}",
                label,
                aliases,
                localized_flat(&concept.description)
            ),
        );
    }
    for cs in ir.code_systems() {
        emit(
            EntityKind::CodeSystem,
            cs.id.as_str(),
            cs.name.clone(),
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
                EntityKind::CodedValue,
                cv.id.as_str(),
                cv.code.clone(),
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
            EntityKind::ValueSet,
            vs.id.as_str(),
            vs.name.clone(),
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
            EntityKind::NotationPattern,
            np.id.as_str(),
            np.name.clone(),
            format!(
                "{} {} {}",
                np.name,
                localized_flat(&np.display_name),
                localized_flat(&np.description)
            ),
        );
    }
    for term in ir.glossary() {
        let aliases: String = term
            .aliases
            .iter()
            .map(localized_flat)
            .collect::<Vec<_>>()
            .join(" ");
        // The glossary term's `term` field is the canonical
        // short form — the operator-facing label. `display_name`
        // is a longer human-readable variant; `description` is
        // the prose definition. Picking `term` for the label
        // column keeps the label-match boost firing on the
        // semantic anchor an operator would type.
        let term_label = localized_flat(&term.term);
        emit(
            EntityKind::GlossaryTerm,
            term.id.as_str(),
            term_label,
            format!(
                "{} {} {} {}",
                localized_flat(&term.term),
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
            (version_id, entity_kind, logical_id, label, doc, tsv) \
         SELECT $1, k::ontology_entity_kind, l, lab, d, to_tsvector('simple', d) \
         FROM UNNEST($2::text[], $3::text[], $4::text[], $5::text[]) AS s(k, l, lab, d)",
    )
    .bind(version_id)
    .bind(&kinds)
    .bind(&lids)
    .bind(&labels)
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
    let mut concepts: Vec<ox_ontology::concept::ConceptDef> = Vec::new();
    let mut glossary: Vec<ox_ontology::glossary::GlossaryTermDef> = Vec::new();
    let mut code_systems: Vec<ox_ontology::code_system::CodeSystemDef> = Vec::new();
    let mut value_sets: Vec<ox_ontology::value_set::ValueSetDef> = Vec::new();
    let mut notation_patterns: Vec<ox_ontology::notation_pattern::NotationPatternDef> = Vec::new();
    let mut concept_maps: Vec<ox_ontology::concept_map::ConceptMapDef> = Vec::new();
    let mut value_range_sets: Vec<ox_ontology::value_range::ValueRangeSetDef> = Vec::new();
    let mut column_profiles: Vec<ox_ontology::column_profile::ColumnProfileDef> = Vec::new();
    let mut segments: Vec<ox_ontology::segment::SegmentDef> = Vec::new();
    let mut table_inventory: Vec<ox_ontology::table_inventory::TableInventoryEntry> = Vec::new();

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
            EntityKind::Provenance => provenance.push(serde_json::from_value(row.content.clone())?),
            EntityKind::Function => functions.push(serde_json::from_value(row.content.clone())?),
            EntityKind::Metric => metrics.push(serde_json::from_value(row.content.clone())?),
            EntityKind::Enrichment => {
                enrichments.push(serde_json::from_value(row.content.clone())?)
            }
            EntityKind::Concept => concepts.push(serde_json::from_value(row.content.clone())?),
            EntityKind::GlossaryTerm => glossary.push(serde_json::from_value(row.content.clone())?),
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
            EntityKind::ColumnProfile => {
                column_profiles.push(serde_json::from_value(row.content.clone())?)
            }
            EntityKind::Segment => segments.push(serde_json::from_value(row.content.clone())?),
            EntityKind::TableInventory => {
                table_inventory.push(serde_json::from_value(row.content.clone())?)
            }
            // Property + CodedValue are nested-only entity kinds —
            // they appear in the materialised navigation / search
            // tables but `extract_entities` never emits them as
            // top-level rows (they live inside their parent's
            // payload). Hitting them in `assemble_ir` would mean the
            // content-addressed store grew a row at this granularity,
            // which is a contract violation. Surface loudly.
            EntityKind::Property | EntityKind::CodedValue => {
                return Err(OxError::Runtime {
                    message: format!(
                        "ontology_entity_versions row has nested-only entity_kind \
                         '{}' — these live inside their parent type's payload and \
                         must never be persisted as standalone entities",
                        row.entity_kind,
                    ),
                });
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
        display_name: ox_core::i18n::LocalizedText,
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
    ir.display_name = h.display_name;

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
    for concept in concepts {
        ir.add_concept(concept).map_err(|e| OxError::Runtime {
            message: format!("add_concept during hydration: {e:?}"),
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
    for cp in column_profiles {
        // `add_column_profile` is upsert-by-id and infallible — no
        // OntologyInvariantError shape exists for this collection.
        ir.add_column_profile(cp);
    }
    for seg in segments {
        ir.add_segment(seg).map_err(|e| OxError::Runtime {
            message: format!("add_segment during hydration: {e:?}"),
        })?;
    }
    for entry in table_inventory {
        ir.upsert_table_inventory_entry(entry)
            .map_err(|e| OxError::Runtime {
                message: format!("upsert_table_inventory_entry during hydration: {e:?}"),
            })?;
    }

    Ok(ir)
}
