use ox_core::diagnostic::{DiagnosticMessage, diag};
use ox_core::types::PropertyType;

use super::cycles::find_cycle;
use super::{
    AggregationRole, EdgeKind, IndexDef, NodeConstraint, NodeTypeDef, NodeTypeId, OntologyIR,
    PropertyDef, PropertyId,
};
use crate::binding::PropertyBinding;
use crate::glossary::{GlossaryTermId, TermLifecycle, TermRelationKind};

/// Sentinel label produced by [`NodeTypeDef::default`] /
/// [`EdgeTypeDef::default`] — a placeholder that satisfies
/// [`ox_core::graph_label::GraphLabel`] invariants but is not a real
/// user label. Reject it at validate() so a caller that forgot to
/// override `label: ...` in struct-update syntax gets a clear error
/// instead of a silent default.
const LABEL_PLACEHOLDER: &str = "__default_placeholder__";

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

fn validate_property_defs(
    owner_kind: &str,
    owner_label: &str,
    properties: &[PropertyDef],
    errors: &mut Vec<DiagnosticMessage>,
) {
    let mut seen_ids = std::collections::HashSet::<&PropertyId>::new();
    let mut seen_names = std::collections::HashSet::new();

    for property in properties {
        if property.id.trim().is_empty() {
            errors.push(
                diag("ontology.validate.property.empty_id")
                    .with("owner_kind", owner_kind)
                    .with("owner_label", owner_label)
                    .message(format!(
                        "{owner_kind} '{owner_label}' has a property with an empty id"
                    )),
            );
        } else if !seen_ids.insert(&property.id) {
            errors.push(
                diag("ontology.validate.property.duplicate_id")
                    .with("owner_kind", owner_kind)
                    .with("owner_label", owner_label)
                    .with("id", property.id.as_str())
                    .message(format!(
                        "{owner_kind} '{owner_label}' has duplicate property id '{}'",
                        property.id
                    )),
            );
        }

        // `PropertyKey` enforces the non-empty / Cypher-safe invariants
        // at construction; only the placeholder sentinel and duplicate
        // detection need checking here.
        let name = property.name.as_str();
        if name == LABEL_PLACEHOLDER {
            errors.push(
                diag("ontology.validate.property.placeholder_name")
                    .with("owner_kind", owner_kind)
                    .with("owner_label", owner_label)
                    .message(format!(
                        "{owner_kind} '{owner_label}' has a property with the \
                         `Default::default()` placeholder name — struct-update \
                         callers must override `name` explicitly"
                    )),
            );
            continue;
        }

        if !seen_names.insert(name.to_string()) {
            errors.push(
                diag("ontology.validate.property.duplicate_name")
                    .with("owner_kind", owner_kind)
                    .with("owner_label", owner_label)
                    .with("name", name)
                    .message(format!(
                        "{owner_kind} '{owner_label}' has duplicate property '{name}'"
                    )),
            );
        }

        // Π-1: AggregationRole sanity — Measure on a non-numeric
        // property_type is almost certainly wrong (you can't SUM a
        // string). Flagged as a validation warning; LLM prompt
        // context would otherwise propose nonsensical aggregations.
        if matches!(property.aggregation_role, Some(AggregationRole::Measure))
            && !matches!(
                property.property_type,
                PropertyType::Int | PropertyType::Float
            )
        {
            errors.push(
                diag("ontology.validate.property.measure_non_numeric")
                    .with("owner_kind", owner_kind)
                    .with("owner_label", owner_label)
                    .with("name", name)
                    .with("property_type", format!("{:?}", property.property_type))
                    .message(format!(
                        "{owner_kind} '{owner_label}' property '{name}' has \
                         aggregation_role=Measure but a non-numeric property_type; \
                         Measure implies SUM/AVG/MAX semantics"
                    )),
            );
        }

        // Mapping-binding coverage: a property that materialises from
        // a physical source (`source_column`) must travel with its
        // meaning. An empty `bindings` list flags a value that
        // arrives without a value-set / code-system / notation /
        // concept binding — the kind of silent semantic drop the IR
        // is meant to prevent. `binding_exempt` is the explicit
        // opt-out for legitimate cases (PK / audit timestamp /
        // opaque id) so an audit can find every exemption by name
        // instead of reading commit history. `Identifier`-role
        // properties get an implicit exemption: they're identity by
        // design and the IR already names that role separately.
        let is_identifier = matches!(property.aggregation_role, Some(AggregationRole::Identifier));
        if property.source_column.is_some()
            && property.bindings.is_empty()
            && property.binding_exempt.is_none()
            && !is_identifier
        {
            errors.push(
                diag("ontology.validate.property.mapping_without_binding")
                    .with("owner_kind", owner_kind)
                    .with("owner_label", owner_label)
                    .with("name", name)
                    .message(format!(
                        "{owner_kind} '{owner_label}' property '{name}' has a \
                         physical mapping (`source_column`) but no semantic \
                         binding — declare a PropertyBinding or set \
                         `binding_exempt` with the reason"
                    )),
            );
        }
    }
}

fn property_def_by_id<'a>(properties: &'a [PropertyDef], id: &str) -> Option<&'a PropertyDef> {
    properties.iter().find(|property| property.id == id)
}

fn validate_constraint_fields(
    node: &NodeTypeDef,
    property_ids: &[PropertyId],
    constraint_name: &str,
    require_non_nullable: bool,
    errors: &mut Vec<DiagnosticMessage>,
) {
    if property_ids.is_empty() {
        errors.push(
            diag("ontology.validate.constraint.empty_property_list")
                .with("node_label", node.label.as_str())
                .with("constraint", constraint_name)
                .message(format!(
                    "Node '{}' has an empty {constraint_name} constraint",
                    node.label
                )),
        );
        return;
    }

    let mut seen = std::collections::HashSet::<&str>::new();
    for prop_id in property_ids {
        let id = prop_id.trim();
        if id.is_empty() {
            errors.push(
                diag("ontology.validate.constraint.empty_property_id")
                    .with("node_label", node.label.as_str())
                    .with("constraint", constraint_name)
                    .message(format!(
                        "Node '{}' has a {constraint_name} constraint with an empty property id",
                        node.label
                    )),
            );
            continue;
        }

        if !seen.insert(id) {
            errors.push(
                diag("ontology.validate.constraint.duplicate_property_id")
                    .with("node_label", node.label.as_str())
                    .with("constraint", constraint_name)
                    .with("property_id", id)
                    .message(format!(
                        "Node '{}' has duplicate property id '{id}' in a {constraint_name} constraint",
                        node.label
                    )),
            );
        }

        match property_def_by_id(&node.properties, id) {
            Some(def) => {
                if require_non_nullable && def.nullable {
                    errors.push(
                        diag("ontology.validate.constraint.requires_non_nullable")
                            .with("node_label", node.label.as_str())
                            .with("constraint", constraint_name)
                            .with("property", def.name.as_str())
                            .message(format!(
                                "Node '{}' constraint '{constraint_name}' requires non-nullable property '{}'",
                                node.label, def.name
                            )),
                    );
                }
            }
            None => errors.push(
                diag("ontology.validate.constraint.unknown_property_id")
                    .with("node_label", node.label.as_str())
                    .with("constraint", constraint_name)
                    .with("property_id", id)
                    .message(format!(
                        "Node '{}' constraint references unknown property id '{id}'",
                        node.label
                    )),
            ),
        }
    }
}

fn validate_index_target(
    node_types: &[NodeTypeDef],
    node_id: &NodeTypeId,
    property_ids: &[PropertyId],
    index_name: &str,
    errors: &mut Vec<DiagnosticMessage>,
) {
    let Some(node) = node_types.iter().find(|node| node.id == *node_id) else {
        errors.push(
            diag("ontology.validate.index.unknown_node_id")
                .with("index", index_name)
                .with("node_id", node_id.as_str())
                .message(format!(
                    "Index '{index_name}' references unknown node id '{node_id}'"
                )),
        );
        return;
    };

    if property_ids.is_empty() {
        errors.push(
            diag("ontology.validate.index.empty_property_list")
                .with("index", index_name)
                .with("node_label", node.label.as_str())
                .message(format!(
                    "Index '{index_name}' on node '{}' must reference at least one property",
                    node.label
                )),
        );
        return;
    }

    let mut seen = std::collections::HashSet::<&str>::new();
    for prop_id in property_ids {
        let id = prop_id.trim();
        if id.is_empty() {
            errors.push(
                diag("ontology.validate.index.empty_property_id")
                    .with("index", index_name)
                    .with("node_label", node.label.as_str())
                    .message(format!(
                        "Index '{index_name}' on node '{}' contains an empty property id",
                        node.label
                    )),
            );
            continue;
        }

        if !seen.insert(id) {
            errors.push(
                diag("ontology.validate.index.duplicate_property_id")
                    .with("index", index_name)
                    .with("node_label", node.label.as_str())
                    .with("property_id", id)
                    .message(format!(
                        "Index '{index_name}' on node '{}' contains duplicate property id '{id}'",
                        node.label
                    )),
            );
        }

        if property_def_by_id(&node.properties, id).is_none() {
            errors.push(
                diag("ontology.validate.index.unknown_property_id")
                    .with("index", index_name)
                    .with("node_label", node.label.as_str())
                    .with("property_id", id)
                    .message(format!(
                        "Index '{index_name}' references unknown property id '{id}' on node '{}'",
                        node.label
                    )),
            );
        }
    }
}

fn validate_column_ref(
    errors: &mut Vec<DiagnosticMessage>,
    code: &'static str,
    link_mapping_id: &str,
    field: &'static str,
    column_ref: &crate::mapping::ColumnRef,
) {
    if column_ref.relation.trim().is_empty() || column_ref.column.trim().is_empty() {
        errors.push(
            diag(code)
                .with("link_mapping_id", link_mapping_id)
                .with("field", field)
                .with("relation", column_ref.relation.as_str())
                .with("column", column_ref.column.as_str())
                .message(format!(
                    "Link mapping '{link_mapping_id}' has an invalid {field} column reference"
                )),
        );
    }
}

fn validate_endpoint_ref(
    errors: &mut Vec<DiagnosticMessage>,
    link_mapping_id: &str,
    role: &'static str,
    endpoint: &crate::mapping::EndpointRef,
) {
    if endpoint.source_id.trim().is_empty() {
        errors.push(
            diag("ontology.validate.link_mapping.empty_endpoint_source_id")
                .with("link_mapping_id", link_mapping_id)
                .with("endpoint", role)
                .message(format!(
                    "Link mapping '{link_mapping_id}' {role} endpoint has an empty source id"
                )),
        );
    }

    if endpoint.relation.trim().is_empty() {
        errors.push(
            diag("ontology.validate.link_mapping.empty_endpoint_relation")
                .with("link_mapping_id", link_mapping_id)
                .with("endpoint", role)
                .message(format!(
                    "Link mapping '{link_mapping_id}' {role} endpoint has an empty relation"
                )),
        );
    }

    if endpoint.key_columns.is_empty() {
        errors.push(
            diag("ontology.validate.link_mapping.empty_endpoint_key")
                .with("link_mapping_id", link_mapping_id)
                .with("endpoint", role)
                .message(format!(
                    "Link mapping '{link_mapping_id}' {role} endpoint must declare at least one key column"
                )),
        );
        return;
    }

    let mut seen = std::collections::HashSet::new();
    for key_column in &endpoint.key_columns {
        if key_column.trim().is_empty() {
            errors.push(
                diag("ontology.validate.link_mapping.empty_endpoint_key_column")
                    .with("link_mapping_id", link_mapping_id)
                    .with("endpoint", role)
                    .message(format!(
                        "Link mapping '{link_mapping_id}' {role} endpoint has an empty key column"
                    )),
            );
        } else if !seen.insert(key_column.as_str()) {
            errors.push(
                diag("ontology.validate.link_mapping.duplicate_endpoint_key_column")
                    .with("link_mapping_id", link_mapping_id)
                    .with("endpoint", role)
                    .with("key_column", key_column.as_str())
                    .message(format!(
                        "Link mapping '{link_mapping_id}' {role} endpoint repeats key column '{key_column}'"
                    )),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

impl OntologyIR {
    /// Validate internal consistency of the ontology.
    ///
    /// Each diagnostic is a structured [`DiagnosticMessage`]
    /// (RFC 7807 / gRPC `Status` shape): a stable `code`, an English
    /// `message` rendering, and a `params` map. The FE resolves
    /// `code` + `params` through its i18n catalogue
    /// (`next-intl` ICU MessageFormat); operator logs and the LLM
    /// tool-result channel consume the English `message` rendering.
    pub fn validate(&self) -> Vec<DiagnosticMessage> {
        self.validate_internal(None)
    }

    /// Same as [`Self::validate`] but additionally checks that every
    /// `source_id` referenced by an `ObjectMappingDef` or
    /// `LinkMappingDef` resolves against the supplied set of
    /// registered sources. Callers invoke this from API boundaries
    /// where the `data_sources` row set is loaded — it surfaces
    /// "mapping points at unregistered source" at IR commit time
    /// rather than as a runtime adapter-resolver error.
    pub fn validate_with_sources(
        &self,
        known_sources: &std::collections::HashSet<crate::mapping::SourceId>,
    ) -> Vec<DiagnosticMessage> {
        self.validate_internal(Some(known_sources))
    }

    /// Φ12 — pre-commit gate that enforces every mapping reference
    /// against the captured `SourceContractDef` bank.
    ///
    /// Where [`Self::validate_with_sources`] only checks that
    /// referenced `source_id`s are in the *registered* source set,
    /// this validator additionally requires that the
    /// `(source_id, relation)` pair exists in the contract bank
    /// and that every column the mapping names lives in that
    /// contract's `columns` list. The introspection pipeline is
    /// the only producer of contract rows; the contract is
    /// authoritative for "what the source actually returned the
    /// last time we asked".
    ///
    /// Returns an empty vec when:
    ///
    /// - the mapping bank is empty (a freshly-created ontology
    ///   with no mappings yet),
    /// - no contract has been captured for a referenced source
    ///   *yet* (the operator has registered the source but has
    ///   not run introspection — soft-skip so first-time setup
    ///   does not fail; once the operator introspects, the gate
    ///   becomes enforcing for that source).
    ///
    /// The "soft-skip on first-time setup" rule is intentional —
    /// turning the gate into a hard fail before any contract has
    /// been captured would prevent the bootstrap workflow from
    /// reaching the introspection step that would *populate* the
    /// contract. Once introspection has run once for a source,
    /// the contract becomes the authoritative shape and any
    /// subsequent mapping that drifts is caught.
    ///
    /// Diagnostic codes emitted:
    ///
    /// - `ontology.validate.object_mapping.relation_not_in_source_contract`
    /// - `ontology.validate.object_mapping.column_not_in_source_contract`
    /// - `ontology.validate.object_mapping.primary_key_column_not_in_source_contract`
    /// - `ontology.validate.link_mapping.endpoint_relation_not_in_source_contract`
    /// - `ontology.validate.link_mapping.endpoint_column_not_in_source_contract`
    /// - `ontology.validate.link_mapping.foreign_key_column_not_in_source_contract`
    /// - `ontology.validate.link_mapping.bridge_relation_not_in_source_contract`
    /// - `ontology.validate.link_mapping.bridge_column_not_in_source_contract`
    /// - `ontology.validate.link_mapping.federated_match_column_not_in_source_contract`
    pub fn validate_against_source_contracts(
        &self,
        contracts: &[crate::source_contract::SourceContractDef],
    ) -> Vec<DiagnosticMessage> {
        use std::collections::{HashMap, HashSet};
        let mut errors: Vec<DiagnosticMessage> = Vec::new();

        if contracts.is_empty() {
            return errors;
        }

        // Index: (source_id, relation) → contract.
        let mut by_relation: HashMap<
            (&crate::mapping::SourceId, &str),
            &crate::source_contract::SourceContractDef,
        > = HashMap::with_capacity(contracts.len());
        let mut sources_with_any_contract: HashSet<&crate::mapping::SourceId> =
            HashSet::with_capacity(contracts.len());
        for c in contracts {
            by_relation.insert((&c.source_id, c.relation.as_str()), c);
            sources_with_any_contract.insert(&c.source_id);
        }

        let column_check = |contract: &crate::source_contract::SourceContractDef,
                            column: &str,
                            diag_code: &'static str,
                            extra: &[(&str, &str)],
                            errors: &mut Vec<DiagnosticMessage>| {
            if !contract.has_column(column) {
                let mut msg = diag(diag_code)
                    .with("source_id", contract.source_id.as_str())
                    .with("relation", contract.relation.as_str())
                    .with("column", column);
                for (k, v) in extra {
                    msg = msg.with(*k, *v);
                }
                errors.push(msg.message(format!(
                    "Column '{column}' is not present on source contract \
                         '{}.{}' captured at introspection time. Re-introspect \
                         the source or remove the column from the mapping.",
                    contract.source_id.as_str(),
                    contract.relation,
                )));
            }
        };

        for mapping in &self.object_mappings {
            // Soft-skip when no contract has been captured for the
            // source at all — bootstrap path before first introspection.
            if !sources_with_any_contract.contains(&mapping.source_id) {
                continue;
            }
            let Some(contract) = by_relation
                .get(&(&mapping.source_id, mapping.relation.as_str()))
                .copied()
            else {
                errors.push(
                    diag("ontology.validate.object_mapping.relation_not_in_source_contract")
                        .with("mapping_id", mapping.id.as_str())
                        .with("source_id", mapping.source_id.as_str())
                        .with("relation", mapping.relation.as_str())
                        .message(format!(
                            "Object mapping '{}' targets relation '{}' on source '{}' \
                             which is not present in the captured source contracts. \
                             Re-introspect the source so the contract registers, or \
                             update the mapping to point at an existing relation.",
                            mapping.id, mapping.relation, mapping.source_id,
                        )),
                );
                continue;
            };

            for pk_col in &mapping.primary_key_columns {
                if pk_col.relation != mapping.relation {
                    continue;
                }
                column_check(
                    contract,
                    &pk_col.column,
                    "ontology.validate.object_mapping.primary_key_column_not_in_source_contract",
                    &[("mapping_id", mapping.id.as_str())],
                    &mut errors,
                );
            }

            // Resolve the parent NodeTypeDef once per object mapping
            // — every property mapping in this batch shares it. The
            // type-compat sub-validator (Φ12.5) reads
            // `node_type.properties[i].property_type` for each
            // property_mapping it walks.
            let parent_node = self
                .node_types
                .iter()
                .find(|n| n.id == mapping.node_type_id);
            for prop_mapping in &mapping.property_mappings {
                let property_type = parent_node.and_then(|n| {
                    n.properties
                        .iter()
                        .find(|p| p.id == prop_mapping.property_id)
                        .map(|p| &p.property_type)
                });
                self.validate_property_mapping_columns(
                    contract,
                    mapping.id.as_str(),
                    property_type,
                    prop_mapping,
                    &mut errors,
                );
            }
        }

        for mapping in &self.link_mappings {
            for (role, endpoint) in [
                ("source", &mapping.source_endpoint),
                ("target", &mapping.target_endpoint),
            ] {
                if !sources_with_any_contract.contains(&endpoint.source_id) {
                    continue;
                }
                let Some(contract) = by_relation
                    .get(&(&endpoint.source_id, endpoint.relation.as_str()))
                    .copied()
                else {
                    errors.push(
                        diag(
                            "ontology.validate.link_mapping.endpoint_relation_not_in_source_contract",
                        )
                        .with("mapping_id", mapping.id.as_str())
                        .with("endpoint_role", role)
                        .with("source_id", endpoint.source_id.as_str())
                        .with("relation", endpoint.relation.as_str())
                        .message(format!(
                            "Link mapping '{}' {} endpoint references relation '{}' on \
                             source '{}' which is not present in the captured source contracts.",
                            mapping.id, role, endpoint.relation, endpoint.source_id,
                        )),
                    );
                    continue;
                };
                for col in &endpoint.key_columns {
                    column_check(
                        contract,
                        col,
                        "ontology.validate.link_mapping.endpoint_column_not_in_source_contract",
                        &[("mapping_id", mapping.id.as_str()), ("endpoint_role", role)],
                        &mut errors,
                    );
                }
            }

            self.validate_link_kind_columns(
                mapping,
                &by_relation,
                &sources_with_any_contract,
                &mut errors,
            );
        }

        errors
    }

    fn validate_property_mapping_columns(
        &self,
        contract: &crate::source_contract::SourceContractDef,
        mapping_id: &str,
        property_type: Option<&PropertyType>,
        prop_mapping: &crate::mapping::property::PropertyMappingDef,
        errors: &mut Vec<DiagnosticMessage>,
    ) {
        match &prop_mapping.location {
            crate::mapping::property::PropertyLocation::Column(cref) => {
                if cref.relation == contract.relation {
                    match contract.column(&cref.column) {
                        None => {
                            errors.push(
                                diag(
                                    "ontology.validate.object_mapping.column_not_in_source_contract",
                                )
                                .with("mapping_id", mapping_id)
                                .with("source_id", contract.source_id.as_str())
                                .with("relation", contract.relation.as_str())
                                .with("column", cref.column.as_str())
                                .with("property_key", prop_mapping.property_key.as_str())
                                .message(format!(
                                    "Property '{}' on mapping '{}' binds to column '{}' which \
                                     is not present on source contract '{}.{}'.",
                                    prop_mapping.property_key.as_str(),
                                    mapping_id,
                                    cref.column,
                                    contract.source_id.as_str(),
                                    contract.relation,
                                )),
                            );
                        }
                        Some(column) => {
                            // Φ12.5 — column exists; check that the
                            // source data type categorises into a
                            // bucket compatible with the ontology
                            // property type. `Identity` transform
                            // only — `SqlExpr` / `Concat` / `Derived`
                            // are operator-authored coercions that
                            // intentionally take a lossy or
                            // type-changing path; the validator
                            // would emit false positives for them.
                            if matches!(
                                prop_mapping.transform,
                                crate::mapping::property::PropertyTransform::Identity
                            ) {
                                self.assert_column_type_compatible(
                                    contract,
                                    mapping_id,
                                    property_type,
                                    column,
                                    prop_mapping,
                                    errors,
                                );
                            }
                        }
                    }
                }
            }
            crate::mapping::property::PropertyLocation::JsonPath { root_column, .. } => {
                if !contract.has_column(root_column) {
                    errors.push(
                        diag("ontology.validate.object_mapping.column_not_in_source_contract")
                            .with("mapping_id", mapping_id)
                            .with("source_id", contract.source_id.as_str())
                            .with("relation", contract.relation.as_str())
                            .with("column", root_column.as_str())
                            .with("property_key", prop_mapping.property_key.as_str())
                            .with("location_kind", "json_path")
                            .message(format!(
                                "Property '{}' on mapping '{}' anchors a JSON path on column \
                             '{}' which is not present on source contract '{}.{}'.",
                                prop_mapping.property_key.as_str(),
                                mapping_id,
                                root_column,
                                contract.source_id.as_str(),
                                contract.relation,
                            )),
                    );
                }
            }
        }

        if let crate::mapping::property::PropertyTransform::Concat { parts, .. } =
            &prop_mapping.transform
        {
            for part in parts {
                if part.relation == contract.relation && !contract.has_column(&part.column) {
                    errors.push(
                        diag("ontology.validate.object_mapping.column_not_in_source_contract")
                            .with("mapping_id", mapping_id)
                            .with("source_id", contract.source_id.as_str())
                            .with("relation", contract.relation.as_str())
                            .with("column", part.column.as_str())
                            .with("property_key", prop_mapping.property_key.as_str())
                            .with("location_kind", "concat_part")
                            .message(format!(
                                "Property '{}' on mapping '{}' concat part references column \
                             '{}' which is not present on source contract '{}.{}'.",
                                prop_mapping.property_key.as_str(),
                                mapping_id,
                                part.column,
                                contract.source_id.as_str(),
                                contract.relation,
                            )),
                    );
                }
            }
        }
    }

    fn validate_link_kind_columns(
        &self,
        mapping: &crate::mapping::link::LinkMappingDef,
        by_relation: &std::collections::HashMap<
            (&crate::mapping::SourceId, &str),
            &crate::source_contract::SourceContractDef,
        >,
        sources_with_any_contract: &std::collections::HashSet<&crate::mapping::SourceId>,
        errors: &mut Vec<DiagnosticMessage>,
    ) {
        let resolve = |source_id: &crate::mapping::SourceId, relation: &str| {
            if !sources_with_any_contract.contains(source_id) {
                return None;
            }
            by_relation.get(&(source_id, relation)).copied()
        };

        match &mapping.kind {
            crate::mapping::link::LinkMappingKind::ForeignKey {
                source_column,
                target_column,
            } => {
                self.assert_link_column_in_contract(
                    mapping,
                    "foreign_key_source",
                    &mapping.source_endpoint.source_id,
                    source_column,
                    resolve(&mapping.source_endpoint.source_id, &source_column.relation),
                    "ontology.validate.link_mapping.foreign_key_column_not_in_source_contract",
                    errors,
                );
                self.assert_link_column_in_contract(
                    mapping,
                    "foreign_key_target",
                    &mapping.target_endpoint.source_id,
                    target_column,
                    resolve(&mapping.target_endpoint.source_id, &target_column.relation),
                    "ontology.validate.link_mapping.foreign_key_column_not_in_source_contract",
                    errors,
                );
            }
            crate::mapping::link::LinkMappingKind::Bridge {
                bridge_relation,
                source_join,
                target_join,
                bridge_workspace_scope,
            } => {
                let bridge_contract =
                    resolve(&bridge_relation.source_id, &bridge_relation.relation);
                if sources_with_any_contract.contains(&bridge_relation.source_id)
                    && bridge_contract.is_none()
                {
                    errors.push(
                        diag(
                            "ontology.validate.link_mapping.bridge_relation_not_in_source_contract",
                        )
                        .with("mapping_id", mapping.id.as_str())
                        .with("source_id", bridge_relation.source_id.as_str())
                        .with("relation", bridge_relation.relation.as_str())
                        .message(format!(
                            "Link mapping '{}' bridge relation '{}' on source '{}' is \
                             not present in the captured source contracts.",
                            mapping.id, bridge_relation.relation, bridge_relation.source_id,
                        )),
                    );
                }
                if let Some(contract) = bridge_contract {
                    for col in source_join.iter().chain(target_join.iter()) {
                        if col.relation == bridge_relation.relation
                            && !contract.has_column(&col.column)
                        {
                            errors.push(
                                diag("ontology.validate.link_mapping.bridge_column_not_in_source_contract")
                                    .with("mapping_id", mapping.id.as_str())
                                    .with("source_id", bridge_relation.source_id.as_str())
                                    .with("relation", bridge_relation.relation.as_str())
                                    .with("column", col.column.as_str())
                                    .message(format!(
                                        "Link mapping '{}' bridge join column '{}' is not \
                                         present on source contract '{}.{}'.",
                                        mapping.id,
                                        col.column,
                                        bridge_relation.source_id.as_str(),
                                        bridge_relation.relation,
                                    )),
                            );
                        }
                    }
                    if let Some(scope) = bridge_workspace_scope
                        && scope.relation == bridge_relation.relation
                        && !contract.has_column(&scope.column)
                    {
                        errors.push(
                            diag("ontology.validate.link_mapping.bridge_column_not_in_source_contract")
                                .with("mapping_id", mapping.id.as_str())
                                .with("source_id", bridge_relation.source_id.as_str())
                                .with("relation", bridge_relation.relation.as_str())
                                .with("column", scope.column.as_str())
                                .with("role", "bridge_workspace_scope")
                                .message(format!(
                                    "Link mapping '{}' bridge workspace-scope column '{}' is \
                                     not present on source contract '{}.{}'.",
                                    mapping.id,
                                    scope.column,
                                    bridge_relation.source_id.as_str(),
                                    bridge_relation.relation,
                                )),
                        );
                    }
                }
            }
            crate::mapping::link::LinkMappingKind::Computed { .. } => {
                // Source-dialect predicate — no structural column
                // surface to validate. Adapters reject malformed
                // predicates at runtime.
            }
            crate::mapping::link::LinkMappingKind::Federated {
                source_match_column,
                target_match_column,
            } => {
                self.assert_link_column_in_contract(
                    mapping,
                    "federated_source_match",
                    &mapping.source_endpoint.source_id,
                    source_match_column,
                    resolve(
                        &mapping.source_endpoint.source_id,
                        &source_match_column.relation,
                    ),
                    "ontology.validate.link_mapping.federated_match_column_not_in_source_contract",
                    errors,
                );
                self.assert_link_column_in_contract(
                    mapping,
                    "federated_target_match",
                    &mapping.target_endpoint.source_id,
                    target_match_column,
                    resolve(
                        &mapping.target_endpoint.source_id,
                        &target_match_column.relation,
                    ),
                    "ontology.validate.link_mapping.federated_match_column_not_in_source_contract",
                    errors,
                );
            }
        }
    }

    /// Φ12.5 — emit
    /// `ontology.validate.object_mapping.column_type_incompatible`
    /// when the source column's data-type category does not fit
    /// the ontology property's `PropertyType`. Silent when:
    ///
    /// - no `property_type` was resolved (parent NodeType not
    ///   found, or property id absent — the topology validator
    ///   already flagged that case),
    /// - the source data-type categorises to
    ///   [`crate::source_contract::SourceTypeCategory::Unknown`]
    ///   — the classifier doesn't recognise the spelling, fail-
    ///   open to avoid false positives on dialect extensions.
    fn assert_column_type_compatible(
        &self,
        contract: &crate::source_contract::SourceContractDef,
        mapping_id: &str,
        property_type: Option<&PropertyType>,
        column: &crate::source_contract::ColumnSpec,
        prop_mapping: &crate::mapping::property::PropertyMappingDef,
        errors: &mut Vec<DiagnosticMessage>,
    ) {
        let Some(property_type) = property_type else {
            return;
        };
        let category = column.category();
        if category.is_compatible_with(property_type) {
            return;
        }
        // Compose a stable string for the params (PropertyType
        // serialises through the existing JsonSchema-friendly
        // Serialize impl — short scalar names).
        let property_type_label = match property_type {
            PropertyType::Bool => "bool".to_string(),
            PropertyType::Int => "int".to_string(),
            PropertyType::Float => "float".to_string(),
            PropertyType::String => "string".to_string(),
            PropertyType::Date => "date".to_string(),
            PropertyType::DateTime => "datetime".to_string(),
            PropertyType::Duration => "duration".to_string(),
            PropertyType::Bytes => "bytes".to_string(),
            PropertyType::List { .. } => "list".to_string(),
            PropertyType::Map => "map".to_string(),
        };
        let category_label = format!("{category:?}").to_ascii_lowercase();
        errors.push(
            diag("ontology.validate.object_mapping.column_type_incompatible")
                .with("mapping_id", mapping_id)
                .with("source_id", contract.source_id.as_str())
                .with("relation", contract.relation.as_str())
                .with("column", column.name.as_str())
                .with("source_data_type", column.data_type.as_str())
                .with("source_category", category_label.as_str())
                .with("property_key", prop_mapping.property_key.as_str())
                .with("property_type", property_type_label.as_str())
                .message(format!(
                    "Property '{}' (type {}) on mapping '{}' binds to column '{}' \
                     of source type '{}' which categorises as {} — incompatible. \
                     Either pick a different column, change the property type, or \
                     wrap the value in a `PropertyTransform::SqlExpr` that performs \
                     the explicit coercion.",
                    prop_mapping.property_key.as_str(),
                    property_type_label,
                    mapping_id,
                    column.name,
                    column.data_type,
                    category_label,
                )),
        );
    }

    fn assert_link_column_in_contract(
        &self,
        mapping: &crate::mapping::link::LinkMappingDef,
        role: &str,
        source_id: &crate::mapping::SourceId,
        col: &crate::mapping::refs::ColumnRef,
        contract: Option<&crate::source_contract::SourceContractDef>,
        diag_code: &'static str,
        errors: &mut Vec<DiagnosticMessage>,
    ) {
        let Some(contract) = contract else {
            // Caller already filtered via sources_with_any_contract;
            // a missing contract here means relation-level absence,
            // already reported by the endpoint-relation pass when
            // applicable. Don't double-report.
            return;
        };
        if col.relation == contract.relation && !contract.has_column(&col.column) {
            errors.push(
                diag(diag_code)
                    .with("mapping_id", mapping.id.as_str())
                    .with("role", role)
                    .with("source_id", source_id.as_str())
                    .with("relation", contract.relation.as_str())
                    .with("column", col.column.as_str())
                    .message(format!(
                        "Link mapping '{}' {} column '{}' is not present on source contract \
                         '{}.{}'.",
                        mapping.id,
                        role,
                        col.column,
                        source_id.as_str(),
                        contract.relation,
                    )),
            );
        }
    }

    fn validate_internal(
        &self,
        known_sources: Option<&std::collections::HashSet<crate::mapping::SourceId>>,
    ) -> Vec<DiagnosticMessage> {
        let mut errors: Vec<DiagnosticMessage> = Vec::new();

        if let Some(known) = known_sources {
            for mapping in &self.object_mappings {
                if !known.contains(&mapping.source_id) {
                    errors.push(
                        diag("ontology.validate.object_mapping.unknown_source")
                            .with("mapping_id", mapping.id.as_str())
                            .with("source_id", mapping.source_id.as_str())
                            .with("relation", mapping.relation.as_str())
                            .message(format!(
                                "Object mapping '{}' targets relation '{}' on source '{}' \
                                 which is not registered. Register the data source before \
                                 committing the mapping.",
                                mapping.id, mapping.relation, mapping.source_id
                            )),
                    );
                }
            }
            for mapping in &self.link_mappings {
                for endpoint in [&mapping.source_endpoint, &mapping.target_endpoint] {
                    if !known.contains(&endpoint.source_id) {
                        errors.push(
                            diag("ontology.validate.link_mapping.unknown_source")
                                .with("mapping_id", mapping.id.as_str())
                                .with("source_id", endpoint.source_id.as_str())
                                .message(format!(
                                    "Link mapping '{}' references source '{}' \
                                     which is not registered.",
                                    mapping.id,
                                    endpoint.source_id.as_str()
                                )),
                        );
                    }
                }
            }
        }

        if self.id.trim().is_empty() {
            errors
                .push(diag("ontology.validate.id.empty").message("Ontology id must not be empty"));
        }
        if self.name.trim().is_empty() {
            errors.push(
                diag("ontology.validate.name.empty").message("Ontology name must not be empty"),
            );
        }
        // Ontology must carry SOME content — but the bootstrap wizard
        // commits glossary-only v1s (no topology yet), so node_types
        // is not the only acceptable seed. Accept any populated
        // collection as evidence the ontology has meaning.
        let has_content = !self.node_types.is_empty()
            || !self.edge_types.is_empty()
            || !self.glossary.is_empty()
            || !self.rules.is_empty()
            || !self.code_systems.is_empty()
            || !self.object_mappings.is_empty()
            || !self.link_mappings.is_empty();
        if !has_content {
            errors.push(diag("ontology.validate.no_content").message(
                "Ontology must populate at least one collection \
                     (node_types / edge_types / glossary / rules / code_systems / mappings)",
            ));
        }

        let mut seen_node_ids = std::collections::HashSet::<NodeTypeId>::new();
        let mut seen_node_labels = std::collections::HashSet::new();

        for node in &self.node_types {
            // Validate node id
            if node.id.trim().is_empty() {
                errors.push(
                    diag("ontology.validate.node.empty_id")
                        .message("Node type id must not be empty"),
                );
            } else if !seen_node_ids.insert(node.id.clone()) {
                errors.push(
                    diag("ontology.validate.node.duplicate_id")
                        .with("id", node.id.as_str())
                        .message(format!("Duplicate node type id: '{}'", node.id)),
                );
            }

            // `GraphLabel` already rejects empty / invalid identifiers
            // at construction time, so validation here reduces to: the
            // placeholder sentinel (caller forgot to override the
            // Default::default() label), and duplicate-label detection.
            let label = node.label.as_str();
            if label == LABEL_PLACEHOLDER {
                errors.push(
                    diag("ontology.validate.node.placeholder_label")
                        .with("id", node.id.as_str())
                        .message(format!(
                            "Node type '{}' has the `Default::default()` placeholder label — \
                             struct-update callers must override `label` explicitly",
                            node.id
                        )),
                );
                continue;
            }

            if !seen_node_labels.insert(label.to_string()) {
                errors.push(
                    diag("ontology.validate.node.duplicate_label")
                        .with("label", label)
                        .message(format!("Duplicate node type label: '{label}'")),
                );
            }

            validate_property_defs("Node", label, &node.properties, &mut errors);

            for constraint_def in &node.constraints {
                if constraint_def.id.trim().is_empty() {
                    errors.push(
                        diag("ontology.validate.constraint.empty_id")
                            .with("node_label", label)
                            .message(format!(
                                "Node '{}' has a constraint with an empty id",
                                node.label
                            )),
                    );
                }

                match &constraint_def.constraint {
                    NodeConstraint::Unique { property_ids } => {
                        validate_constraint_fields(
                            node,
                            property_ids,
                            "unique",
                            false,
                            &mut errors,
                        );
                    }
                    NodeConstraint::NodeKey { property_ids } => {
                        validate_constraint_fields(
                            node,
                            property_ids,
                            "node_key",
                            true,
                            &mut errors,
                        );
                    }
                    NodeConstraint::Exists { property_id } => {
                        validate_constraint_fields(
                            node,
                            std::slice::from_ref(property_id),
                            "exists",
                            true,
                            &mut errors,
                        );
                    }
                }
            }
        }

        // Check edge types reference valid node IDs
        let mut seen_edge_signatures = std::collections::HashSet::new();
        for edge in &self.edge_types {
            // Validate edge id
            if edge.id.trim().is_empty() {
                errors.push(
                    diag("ontology.validate.edge.empty_id")
                        .message("Edge type id must not be empty"),
                );
            }

            // Parallel to the node case above: `GraphLabel` enforces
            // the identifier invariants at construction, so validation
            // here only has to catch the sentinel and duplicates.
            let label = edge.label.as_str();
            if label == LABEL_PLACEHOLDER {
                errors.push(
                    diag("ontology.validate.edge.placeholder_label")
                        .with("id", edge.id.as_str())
                        .message(format!(
                            "Edge type '{}' has the `Default::default()` placeholder label — \
                             struct-update callers must override `label` explicitly",
                            edge.id
                        )),
                );
                continue;
            }
            if edge.source_node_id.trim().is_empty() || edge.target_node_id.trim().is_empty() {
                errors.push(
                    diag("ontology.validate.edge.missing_endpoint")
                        .with("label", label)
                        .message(format!(
                            "Edge '{}' must define both source_node_id and target_node_id",
                            edge.label
                        )),
                );
            }
            if !seen_edge_signatures.insert((
                edge.label.clone(),
                edge.source_node_id.clone(),
                edge.target_node_id.clone(),
            )) {
                errors.push(
                    diag("ontology.validate.edge.duplicate_signature")
                        .with("label", label)
                        .with("source_node_id", edge.source_node_id.as_str())
                        .with("target_node_id", edge.target_node_id.as_str())
                        .message(format!(
                            "Duplicate edge type definition: '{}({}->{})'",
                            edge.label, edge.source_node_id, edge.target_node_id
                        )),
                );
            }

            validate_property_defs("Edge", &edge.label, &edge.properties, &mut errors);

            // EdgeKind::Composition implies UML strong ownership: each
            // part belongs to exactly one whole, and the whole's
            // deletion cascades. The cardinality must keep the source
            // (whole) singular per relation instance — `OneToOne` or
            // `OneToMany`. `ManyToOne` (multiple wholes per part) and
            // `ManyToMany` break the ownership contract; reject them
            // at validate so the runtime cascade-delete can rely on
            // the invariant.
            if edge.kind == EdgeKind::Composition && !edge.cardinality.source_is_singular() {
                errors.push(
                    diag("ontology.validate.edge.composition_requires_singular_source")
                        .with("label", label)
                        .with("cardinality", format!("{:?}", edge.cardinality))
                        .message(format!(
                            "Edge '{}' uses EdgeKind::Composition but cardinality \
                             {:?} would let a part have multiple wholes; \
                             composition requires OneToOne or OneToMany",
                            edge.label, edge.cardinality
                        )),
                );
            }

            if !seen_node_ids.contains::<str>(&edge.source_node_id) {
                errors.push(
                    diag("ontology.validate.edge.unknown_source_node_id")
                        .with("label", label)
                        .with("source_node_id", edge.source_node_id.as_str())
                        .message(format!(
                            "Edge '{}' references unknown source node id '{}'",
                            edge.label, edge.source_node_id
                        )),
                );
            }
            if !seen_node_ids.contains::<str>(&edge.target_node_id) {
                errors.push(
                    diag("ontology.validate.edge.unknown_target_node_id")
                        .with("label", label)
                        .with("target_node_id", edge.target_node_id.as_str())
                        .message(format!(
                            "Edge '{}' references unknown target node id '{}'",
                            edge.label, edge.target_node_id
                        )),
                );
            }
        }

        // -------------------------------------------------------------
        // Referential integrity. Every id link on NodeTypeDef /
        // PropertyDef plus every object / link mapping id must
        // resolve against a matching *Def. Lookup indices on
        // `self.lookup` make each check O(1).
        // -------------------------------------------------------------
        for node in &self.node_types {
            for if_id in &node.implements {
                if !self.lookup.interface_id_idx.contains_key(if_id) {
                    errors.push(
                        diag("ontology.validate.node.unknown_interface_id")
                            .with("node_label", node.label.as_str())
                            .with("interface_id", if_id.as_str())
                            .message(format!(
                                "Node '{}' implements unknown interface id '{}'",
                                node.label, if_id
                            )),
                    );
                }
            }
            for act_id in &node.actions {
                if !self.lookup.action_id_idx.contains_key(act_id) {
                    errors.push(
                        diag("ontology.validate.node.unknown_action_id")
                            .with("node_label", node.label.as_str())
                            .with("action_id", act_id.as_str())
                            .message(format!(
                                "Node '{}' references unknown action id '{}'",
                                node.label, act_id
                            )),
                    );
                }
            }
            for met_id in &node.metrics {
                if !self.lookup.metric_id_idx.contains_key(met_id) {
                    errors.push(
                        diag("ontology.validate.node.unknown_metric_id")
                            .with("node_label", node.label.as_str())
                            .with("metric_id", met_id.as_str())
                            .message(format!(
                                "Node '{}' references unknown metric id '{}'",
                                node.label, met_id
                            )),
                    );
                }
            }
            for rule_id in &node.rules {
                if !self.lookup.rule_id_idx.contains_key(rule_id) {
                    errors.push(
                        diag("ontology.validate.node.unknown_rule_id")
                            .with("node_label", node.label.as_str())
                            .with("rule_id", rule_id.as_str())
                            .message(format!(
                                "Node '{}' references unknown rule id '{}'",
                                node.label, rule_id
                            )),
                    );
                }
            }

            // Property-level governance — derived-from checks plus a
            // single walk over `bindings` so every registry-target
            // is checked through the same code path.
            for prop in &node.properties {
                if let Some(fn_id) = &prop.derived_from {
                    if !self.lookup.function_id_idx.contains_key(fn_id) {
                        errors.push(
                            diag("ontology.validate.property.unknown_function_id")
                                .with("node_label", node.label.as_str())
                                .with("property", prop.name.as_str())
                                .with("function_id", fn_id.as_str())
                                .message(format!(
                                    "Property '{}.{}' is derived_from unknown function id '{}'",
                                    node.label, prop.name, fn_id
                                )),
                        );
                    }
                    // derived_from and source_column are mutually
                    // exclusive: a value cannot both be computed and
                    // read from a physical column. The planner refuses
                    // such a PropertyDef at compile time; catching it
                    // here gives authors a clearer error.
                    if prop.source_column.is_some() {
                        errors.push(
                            diag("ontology.validate.property.derived_and_source_conflict")
                                .with("node_label", node.label.as_str())
                                .with("property", prop.name.as_str())
                                .message(format!(
                                    "Property '{}.{}' declares both `derived_from` and \
                                     `source_column` — pick one",
                                    node.label, prop.name
                                )),
                        );
                    }
                }
                // unit_id must point at a CodedValue registered in
                // the global coded_value index. Any CodeSystem can
                // supply units; UCUM is the canonical external
                // system but domain-specific systems are valid too.
                if let Some(unit_id) = &prop.unit_id
                    && !self.lookup.coded_value_loc.contains_key(unit_id)
                {
                    errors.push(
                        diag("ontology.validate.property.unknown_unit_id")
                            .with("node_label", node.label.as_str())
                            .with("property", prop.name.as_str())
                            .with("unit_id", unit_id.as_str())
                            .message(format!(
                                "Property '{}.{}' references unknown unit (CodedValueId '{}')",
                                node.label, prop.name, unit_id
                            )),
                    );
                }
                // Single walk: every binding's target id must
                // resolve in its registry. `Required` strength
                // additionally rejects targets whose domain is
                // empty (would force every write to fail).
                for binding in &prop.bindings {
                    if let Some(msg) =
                        self.check_binding_target_exists(&node.label, prop.name.as_str(), binding)
                    {
                        errors.push(msg);
                    }
                    if let Some(msg) = self.check_binding_concept_map_compatibility(
                        &node.label,
                        prop.name.as_str(),
                        binding,
                    ) {
                        errors.push(msg);
                    }
                    if binding.strength() == crate::binding::BindingStrength::Required
                        && let Some(msg) = self.check_required_binding_domain(
                            &node.label,
                            prop.name.as_str(),
                            binding,
                        )
                    {
                        errors.push(msg);
                    }
                }
            }
        }

        // Cross-check rule activations: `OnAction { action_id }` must
        // reference a real action. A dangling rule activation silently
        // never fires, which is the exact shape of bug this check
        // prevents.
        for rule in &self.rules {
            if let crate::rule::RuleActivationKind::OnAction { action_id } = &rule.activation
                && !self.lookup.action_id_idx.contains_key(action_id)
            {
                errors.push(
                    diag("ontology.validate.rule.unknown_action_id")
                        .with("rule_id", rule.id.as_str())
                        .with("action_id", action_id.as_str())
                        .message(format!(
                            "Rule '{}' activates on unknown action id '{}'",
                            rule.name, action_id
                        )),
                );
            }

            // A `DerivedFromBinding` rule names the (node, property)
            // its constraint was synthesised from. If the source
            // binding has been removed but the derived rule wasn't
            // regenerated, the rule is an orphan that fires on stale
            // semantics. Reject so the unbind path forces a
            // companion rule cleanup, keeping derived state in
            // lock-step with its source.
            if let crate::rule::RuleOrigin::DerivedFromBinding {
                node_type_id,
                property_id,
            } = &rule.origin
            {
                let source_exists = self
                    .lookup
                    .node_id_idx
                    .get(node_type_id)
                    .and_then(|&i| self.node_types.get(i))
                    .and_then(|n| n.properties.iter().find(|p| p.id == *property_id))
                    .map(|p| !p.bindings.is_empty())
                    .unwrap_or(false);
                if !source_exists {
                    errors.push(
                        diag("ontology.validate.rule.derived_origin_missing_binding")
                            .with("rule_id", rule.id.as_str())
                            .with("node_type_id", node_type_id.as_str())
                            .with("property_id", property_id.as_str())
                            .message(format!(
                                "Rule '{}' has origin DerivedFromBinding({}, {}) but \
                                 the source binding has been removed; regenerate \
                                 derived rules or promote this rule to Authored",
                                rule.id, node_type_id, property_id
                            )),
                    );
                }
            }

            // Property-pair constraints (`LessThan` / `Equals`)
            // reference a sibling `other_property` on the same node
            // type that the rule's `PropertyShape` targets. A missing
            // sibling silently turns the rule into a no-op at write
            // time, so reject at validate.
            let crate::rule::RuleKind::PropertyShape {
                target_node_type_id,
                target_property_id: _,
            } = &rule.kind
            else {
                continue;
            };
            let Some(&node_idx) = self.lookup.node_id_idx.get(target_node_type_id) else {
                continue;
            };
            let node = &self.node_types[node_idx];
            for sc in &rule.constraints {
                let other_property_id = match sc {
                    crate::rule::ShaclConstraint::LessThan { other_property, .. }
                    | crate::rule::ShaclConstraint::Equals { other_property, .. } => other_property,
                    _ => continue,
                };
                let exists = node.properties.iter().any(|p| p.id == *other_property_id);
                if !exists {
                    errors.push(
                        diag("ontology.validate.rule.property_pair_unknown_sibling")
                            .with("rule_id", rule.id.as_str())
                            .with("node_label", node.label.as_str())
                            .with("other_property_id", other_property_id.as_str())
                            .message(format!(
                                "Rule '{}' on node '{}' references unknown sibling \
                                 property id '{}' in a property-pair constraint",
                                rule.name, node.label, other_property_id
                            )),
                    );
                }
            }
        }

        // Action precondition / postcondition ids must reference real
        // rules. Same rationale as rule activations.
        for action in &self.actions {
            for rule_id in action
                .preconditions
                .iter()
                .chain(action.postconditions.iter())
            {
                if !self.lookup.rule_id_idx.contains_key(rule_id) {
                    errors.push(
                        diag("ontology.validate.action.unknown_rule_id")
                            .with("action_id", action.id.as_str())
                            .with("rule_id", rule_id.as_str())
                            .message(format!(
                                "Action '{}' references unknown rule id '{}' in its pre/post conditions",
                                action.name, rule_id
                            )),
                    );
                }
            }
        }

        // -------------------------------------------------------------
        // Mapping referential integrity. O(1) per mapping via
        // `node_id_idx` / `edge_id_idx` instead of a per-validate
        // HashSet alloc.
        // -------------------------------------------------------------
        for om in &self.object_mappings {
            let Some(&node_idx) = self.lookup.node_id_idx.get(&om.node_type_id) else {
                errors.push(
                    diag("ontology.validate.object_mapping.unknown_node_type_id")
                        .with("object_mapping_id", om.id.as_str())
                        .with("node_type_id", om.node_type_id.as_str())
                        .message(format!(
                            "Object mapping '{}' targets unknown node type id '{}'",
                            om.id, om.node_type_id
                        )),
                );
                continue;
            };

            let node = &self.node_types[node_idx];
            let property_ids = node
                .properties
                .iter()
                .map(|p| &p.id)
                .collect::<std::collections::HashSet<_>>();
            let mut mapped_property_ids = std::collections::HashSet::new();
            for property_mapping in &om.property_mappings {
                if !mapped_property_ids.insert(&property_mapping.property_id) {
                    errors.push(
                        diag("ontology.validate.object_mapping.duplicate_property_mapping")
                            .with("object_mapping_id", om.id.as_str())
                            .with("node_type_id", om.node_type_id.as_str())
                            .with("property_id", property_mapping.property_id.as_str())
                            .message(format!(
                                "Object mapping '{}' maps property id '{}' more than once",
                                om.id, property_mapping.property_id
                            )),
                    );
                }

                if !property_ids.contains(&property_mapping.property_id) {
                    errors.push(
                        diag("ontology.validate.object_mapping.unknown_property_id")
                            .with("object_mapping_id", om.id.as_str())
                            .with("node_type_id", om.node_type_id.as_str())
                            .with("property_id", property_mapping.property_id.as_str())
                            .message(format!(
                                "Object mapping '{}' targets unknown property id '{}' on node type '{}'",
                                om.id, property_mapping.property_id, om.node_type_id
                            )),
                    );
                }

                if let Some(concept_map_id) = &property_mapping.concept_map_id
                    && !self.lookup.concept_map_id_idx.contains_key(concept_map_id)
                {
                    errors.push(
                        diag("ontology.validate.object_mapping.unknown_concept_map_id")
                            .with("object_mapping_id", om.id.as_str())
                            .with("node_type_id", om.node_type_id.as_str())
                            .with("property_id", property_mapping.property_id.as_str())
                            .with("concept_map_id", concept_map_id.as_str())
                            .message(format!(
                                "Object mapping '{}' property '{}' references unknown concept map id '{}'",
                                om.id, property_mapping.property_id, concept_map_id
                            )),
                    );
                }
            }
        }
        for lm in &self.link_mappings {
            validate_endpoint_ref(&mut errors, lm.id.as_str(), "source", &lm.source_endpoint);
            validate_endpoint_ref(&mut errors, lm.id.as_str(), "target", &lm.target_endpoint);

            if !self.lookup.edge_id_idx.contains_key(&lm.edge_type_id) {
                errors.push(
                    diag("ontology.validate.link_mapping.unknown_edge_type_id")
                        .with("link_mapping_id", lm.id.as_str())
                        .with("edge_type_id", lm.edge_type_id.as_str())
                        .message(format!(
                            "Link mapping '{}' targets unknown edge type id '{}'",
                            lm.id, lm.edge_type_id
                        )),
                );
            }

            match &lm.kind {
                crate::mapping::LinkMappingKind::ForeignKey {
                    source_column,
                    target_column,
                } => {
                    validate_column_ref(
                        &mut errors,
                        "ontology.validate.link_mapping.invalid_foreign_key_column",
                        lm.id.as_str(),
                        "source_column",
                        source_column,
                    );
                    validate_column_ref(
                        &mut errors,
                        "ontology.validate.link_mapping.invalid_foreign_key_column",
                        lm.id.as_str(),
                        "target_column",
                        target_column,
                    );
                }
                crate::mapping::LinkMappingKind::Bridge {
                    bridge_relation,
                    source_join,
                    target_join,
                    bridge_workspace_scope,
                } => {
                    if bridge_relation.source_id.trim().is_empty()
                        || bridge_relation.relation.trim().is_empty()
                    {
                        errors.push(
                            diag("ontology.validate.link_mapping.invalid_bridge_relation")
                                .with("link_mapping_id", lm.id.as_str())
                                .with("source_id", bridge_relation.source_id.as_str())
                                .with("relation", bridge_relation.relation.as_str())
                                .message(format!(
                                    "Link mapping '{}' has an invalid bridge relation",
                                    lm.id
                                )),
                        );
                    }
                    if source_join.len() != lm.source_endpoint.key_columns.len() {
                        errors.push(
                            diag("ontology.validate.link_mapping.source_bridge_join_arity_mismatch")
                                .with("link_mapping_id", lm.id.as_str())
                                .with("join_columns", source_join.len().to_string())
                                .with(
                                    "endpoint_key_columns",
                                    lm.source_endpoint.key_columns.len().to_string(),
                                )
                                .message(format!(
                                    "Link mapping '{}' source bridge join arity does not match source endpoint key arity",
                                    lm.id
                                )),
                        );
                    }
                    if target_join.len() != lm.target_endpoint.key_columns.len() {
                        errors.push(
                            diag("ontology.validate.link_mapping.target_bridge_join_arity_mismatch")
                                .with("link_mapping_id", lm.id.as_str())
                                .with("join_columns", target_join.len().to_string())
                                .with(
                                    "endpoint_key_columns",
                                    lm.target_endpoint.key_columns.len().to_string(),
                                )
                                .message(format!(
                                    "Link mapping '{}' target bridge join arity does not match target endpoint key arity",
                                    lm.id
                                )),
                        );
                    }
                    for column_ref in source_join {
                        validate_column_ref(
                            &mut errors,
                            "ontology.validate.link_mapping.invalid_bridge_join_column",
                            lm.id.as_str(),
                            "source_join",
                            column_ref,
                        );
                    }
                    for column_ref in target_join {
                        validate_column_ref(
                            &mut errors,
                            "ontology.validate.link_mapping.invalid_bridge_join_column",
                            lm.id.as_str(),
                            "target_join",
                            column_ref,
                        );
                    }
                    if let Some(column_ref) = bridge_workspace_scope {
                        validate_column_ref(
                            &mut errors,
                            "ontology.validate.link_mapping.invalid_bridge_workspace_scope",
                            lm.id.as_str(),
                            "bridge_workspace_scope",
                            column_ref,
                        );
                    }
                }
                crate::mapping::LinkMappingKind::Computed { predicate } => {
                    if predicate.trim().is_empty() {
                        errors.push(
                            diag("ontology.validate.link_mapping.empty_computed_predicate")
                                .with("link_mapping_id", lm.id.as_str())
                                .message(format!(
                                    "Link mapping '{}' computed predicate must not be empty",
                                    lm.id
                                )),
                        );
                    }
                }
                crate::mapping::LinkMappingKind::Federated {
                    source_match_column,
                    target_match_column,
                } => {
                    validate_column_ref(
                        &mut errors,
                        "ontology.validate.link_mapping.invalid_federated_match_column",
                        lm.id.as_str(),
                        "source_match_column",
                        source_match_column,
                    );
                    validate_column_ref(
                        &mut errors,
                        "ontology.validate.link_mapping.invalid_federated_match_column",
                        lm.id.as_str(),
                        "target_match_column",
                        target_match_column,
                    );
                }
            }
            // Π-2: cardinality sanity. A `Bridge` link claiming
            // anything other than `ManyToMany` is almost certainly
            // author error — bridges exist precisely because the
            // underlying relationship is many-to-many. Bridges in
            // practice can be narrower, but we flag as advisory so
            // the author sees it in the admin UI and can either
            // confirm or fix.
            if matches!(lm.kind, crate::mapping::LinkMappingKind::Bridge { .. })
                && lm.cardinality != crate::mapping::LinkCardinality::ManyToMany
            {
                errors.push(
                    diag("ontology.validate.link_mapping.bridge_cardinality_mismatch")
                        .with("link_mapping_id", lm.id.as_str())
                        .with("cardinality", format!("{:?}", lm.cardinality))
                        .message(format!(
                            "Link mapping '{}' uses Bridge kind but declares \
                             cardinality={:?}; Bridge typically implies \
                             ManyToMany — verify the override is intentional",
                            lm.id, lm.cardinality
                        )),
                );
            }
        }

        for index in &self.indexes {
            match index {
                IndexDef::Single {
                    id: _,
                    node_id,
                    property_id,
                } => validate_index_target(
                    &self.node_types,
                    node_id,
                    std::slice::from_ref(property_id),
                    "single",
                    &mut errors,
                ),
                IndexDef::Composite {
                    id: _,
                    node_id,
                    property_ids,
                } => validate_index_target(
                    &self.node_types,
                    node_id,
                    property_ids,
                    "composite",
                    &mut errors,
                ),
                IndexDef::FullText {
                    id: _,
                    name,
                    node_id,
                    property_ids,
                } => {
                    // `GraphLabel` rejects empty / invalid names at
                    // construction, so nothing left to check here
                    // except the target node / property references.
                    validate_index_target(
                        &self.node_types,
                        node_id,
                        property_ids,
                        name.as_str(),
                        &mut errors,
                    );
                }
                IndexDef::Vector {
                    id: _,
                    node_id,
                    property_id,
                    dimensions,
                    ..
                } => {
                    if *dimensions == 0 {
                        errors.push(
                            diag("ontology.validate.index.vector_zero_dimensions")
                                .with("node_id", node_id.as_str())
                                .with("property_id", property_id.as_str())
                                .message(format!(
                                    "Vector index on node '{}' property '{}' must have dimensions > 0",
                                    node_id, property_id
                                )),
                        );
                    }
                    validate_index_target(
                        &self.node_types,
                        node_id,
                        std::slice::from_ref(property_id),
                        "vector",
                        &mut errors,
                    );
                }
            }
        }

        // -------------------------------------------------------------
        // Registry cross-reference integrity. The dedicated module
        // walks every `Option<XxxId>` pointer on PropertyDef /
        // RuleDef / ValueSetDef / ConceptMapDef and flags any id
        // that does not resolve.
        use crate::integrity::{RegistryReferenceCheck, render_dangling_references};
        let dangling = self.dangling_references();
        if !dangling.is_empty() {
            errors.extend(render_dangling_references(&dangling));
        }

        // -------------------------------------------------------------
        // Table-inventory referential integrity. Every
        // `contributed_node_ids` entry must resolve to a declared
        // NodeType, and every `contributed_edge_ids` to a declared
        // EdgeType.
        let known_node_ids: std::collections::HashSet<&str> =
            self.node_types.iter().map(|n| n.id.as_str()).collect();
        let known_edge_ids: std::collections::HashSet<&str> =
            self.edge_types.iter().map(|e| e.id.as_str()).collect();
        for entry in &self.table_inventory {
            for nid in &entry.contributed_node_ids {
                if !known_node_ids.contains(nid.as_str()) {
                    errors.push(
                        diag("ontology.validate.table_inventory.unknown_node_type")
                            .with("source_id", entry.source_id.as_str())
                            .with("table_name", entry.table_name.as_str())
                            .with("node_type_id", nid.as_str())
                            .message(format!(
                                "Table inventory entry '{}/{}' references node type '{}' which is not declared",
                                entry.source_id, entry.table_name, nid
                            )),
                    );
                }
            }
            for eid in &entry.contributed_edge_ids {
                if !known_edge_ids.contains(eid.as_str()) {
                    errors.push(
                        diag("ontology.validate.table_inventory.unknown_edge_type")
                            .with("source_id", entry.source_id.as_str())
                            .with("table_name", entry.table_name.as_str())
                            .with("edge_type_id", eid.as_str())
                            .message(format!(
                                "Table inventory entry '{}/{}' references edge type '{}' which is not declared",
                                entry.source_id, entry.table_name, eid
                            )),
                    );
                }
            }
        }

        // -------------------------------------------------------------
        // Concept-term referential integrity.
        //
        // A glossary term whose `realisation` is `Some(_)` is a
        // workspace-canonical business concept. The realisation must
        // resolve to a real Segment / Function on this same IR.
        let known_segments: std::collections::HashSet<&str> =
            self.segments.iter().map(|s| s.id.as_str()).collect();
        let known_functions: std::collections::HashSet<&str> =
            self.functions.iter().map(|f| f.id.as_str()).collect();

        // Concept ↔ glossary referential integrity. Every
        // ConceptDef pins a canonical lexical realisation
        // (`canonical_term_id`) and optionally fans out to
        // alias terms (`alias_term_ids`); both sides must
        // resolve to glossary entries that actually exist on
        // this IR and the glossary entries must point back to the
        // same concept. Without this check a stale lexical term can
        // make concept-binding suggestions attach properties to the
        // wrong business concept.
        // `broader` (concept hierarchy parent) must resolve to
        // another concept on the same IR.
        let glossary_index: std::collections::HashMap<
            &GlossaryTermId,
            &crate::glossary::GlossaryTermDef,
        > = self.glossary.iter().map(|t| (&t.id, t)).collect();
        let known_term_ids: std::collections::HashSet<&GlossaryTermId> =
            self.glossary.iter().map(|t| &t.id).collect();
        let known_concept_ids: std::collections::HashSet<&str> =
            self.concepts.iter().map(|c| c.id.as_str()).collect();
        let concept_index: std::collections::HashMap<
            &crate::concept::ConceptId,
            &crate::concept::ConceptDef,
        > = self.concepts.iter().map(|c| (&c.id, c)).collect();
        let mut claimed_terms =
            std::collections::HashMap::<&GlossaryTermId, &crate::concept::ConceptId>::new();
        for concept in &self.concepts {
            let mut concept_terms = std::collections::HashSet::<&GlossaryTermId>::new();
            concept_terms.insert(&concept.canonical_term_id);
            if let Some(term) = glossary_index.get(&concept.canonical_term_id) {
                if term.concept_id.as_ref() != Some(&concept.id) {
                    errors.push(
                        diag("ontology.validate.concept.canonical_term_concept_mismatch")
                            .with("concept_id", concept.id.as_str())
                            .with("term_id", concept.canonical_term_id.as_str())
                            .with(
                                "term_concept_id",
                                term.concept_id.as_ref().map(|id| id.as_str()).unwrap_or(""),
                            )
                            .message(format!(
                                "Concept '{}' canonical term '{}' must carry concept_id '{}'",
                                concept.id, concept.canonical_term_id, concept.id
                            )),
                    );
                }
                if let Some(previous) =
                    claimed_terms.insert(&concept.canonical_term_id, &concept.id)
                    && previous != &concept.id
                {
                    errors.push(
                        diag("ontology.validate.concept.term_reused")
                            .with("concept_id", concept.id.as_str())
                            .with("previous_concept_id", previous.as_str())
                            .with("term_id", concept.canonical_term_id.as_str())
                            .message(format!(
                                "Concept '{}' reuses glossary term '{}' already claimed by concept '{}'",
                                concept.id, concept.canonical_term_id, previous
                            )),
                    );
                }
            } else if !known_term_ids.contains(&concept.canonical_term_id) {
                errors.push(
                    diag("ontology.validate.concept.unknown_canonical_term")
                        .with("concept_id", concept.id.as_str())
                        .with("term_id", concept.canonical_term_id.as_str())
                        .message(format!(
                            "Concept '{}' canonical_term_id '{}' \
                             does not resolve to a glossary entry",
                            concept.id, concept.canonical_term_id
                        )),
                );
            }
            for alias_id in &concept.alias_term_ids {
                if !concept_terms.insert(alias_id) {
                    errors.push(
                        diag("ontology.validate.concept.duplicate_lexical_term")
                            .with("concept_id", concept.id.as_str())
                            .with("term_id", alias_id.as_str())
                            .message(format!(
                                "Concept '{}' declares glossary term '{}' more than once",
                                concept.id, alias_id
                            )),
                    );
                }
                if let Some(term) = glossary_index.get(alias_id) {
                    if term.concept_id.as_ref() != Some(&concept.id) {
                        errors.push(
                            diag("ontology.validate.concept.alias_term_concept_mismatch")
                                .with("concept_id", concept.id.as_str())
                                .with("term_id", alias_id.as_str())
                                .with(
                                    "term_concept_id",
                                    term.concept_id.as_ref().map(|id| id.as_str()).unwrap_or(""),
                                )
                                .message(format!(
                                    "Concept '{}' alias term '{}' must carry concept_id '{}'",
                                    concept.id, alias_id, concept.id
                                )),
                        );
                    }
                    if let Some(previous) = claimed_terms.insert(alias_id, &concept.id)
                        && previous != &concept.id
                    {
                        errors.push(
                            diag("ontology.validate.concept.term_reused")
                                .with("concept_id", concept.id.as_str())
                                .with("previous_concept_id", previous.as_str())
                                .with("term_id", alias_id.as_str())
                                .message(format!(
                                    "Concept '{}' reuses glossary term '{}' already claimed by concept '{}'",
                                    concept.id, alias_id, previous
                                )),
                        );
                    }
                } else if !known_term_ids.contains(alias_id) {
                    errors.push(
                        diag("ontology.validate.concept.unknown_alias_term")
                            .with("concept_id", concept.id.as_str())
                            .with("term_id", alias_id.as_str())
                            .message(format!(
                                "Concept '{}' alias_term_ids includes '{}' \
                                 which is not a glossary entry",
                                concept.id, alias_id
                            )),
                    );
                }
            }
            if let Some(parent) = &concept.broader
                && !known_concept_ids.contains(parent.as_str())
            {
                errors.push(
                    diag("ontology.validate.concept.unknown_broader")
                        .with("concept_id", concept.id.as_str())
                        .with("broader_id", parent.as_str())
                        .message(format!(
                            "Concept '{}' broader pointer '{}' does not \
                             resolve to a sibling concept",
                            concept.id, parent
                        )),
                );
            }
            if let Some(target) = &concept.replaced_by
                && !known_concept_ids.contains(target.as_str())
            {
                errors.push(
                    diag("ontology.validate.concept.unknown_replaced_by")
                        .with("concept_id", concept.id.as_str())
                        .with("replaced_by_id", target.as_str())
                        .message(format!(
                            "Concept '{}' replaced_by pointer '{}' does not \
                             resolve to a sibling concept",
                            concept.id, target
                        )),
                );
            }
            if let (Some(from), Some(to)) = (concept.valid_from, concept.valid_to)
                && from >= to
            {
                errors.push(
                    diag("ontology.validate.concept.invalid_validity_window")
                        .with("concept_id", concept.id.as_str())
                        .with("valid_from", from.to_rfc3339())
                        .with("valid_to", to.to_rfc3339())
                        .message(format!(
                            "Concept '{}' valid_from ({}) is not strictly before valid_to ({})",
                            concept.id,
                            from.to_rfc3339(),
                            to.to_rfc3339()
                        )),
                );
            }
        }
        for term in &self.glossary {
            if let Some(concept_id) = &term.concept_id
                && !known_concept_ids.contains(concept_id.as_str())
            {
                errors.push(
                    diag("ontology.validate.glossary_term.unknown_concept")
                        .with("term_id", term.id.as_str())
                        .with("concept_id", concept_id.as_str())
                        .message(format!(
                            "Glossary term '{}' concept_id '{}' does not resolve to a concept",
                            term.id, concept_id
                        )),
                );
            }
        }

        for concept in &self.concepts {
            match &concept.realisation {
                Some(crate::glossary::TermRealisation::Segment { segment_id })
                    if !known_segments.contains(segment_id.as_str()) =>
                {
                    errors.push(
                        diag("ontology.validate.concept.unknown_realisation_segment")
                            .with("concept_id", concept.id.as_str())
                            .with("segment_id", segment_id.as_str())
                            .message(format!(
                                "Concept '{}' realisation references segment '{}' which is not declared",
                                concept.id, segment_id
                            )),
                    );
                }
                Some(crate::glossary::TermRealisation::Function { function_id })
                    if !known_functions.contains(function_id.as_str()) =>
                {
                    errors.push(
                        diag("ontology.validate.concept.unknown_realisation_function")
                            .with("concept_id", concept.id.as_str())
                            .with("function_id", function_id.as_str())
                            .message(format!(
                                "Concept '{}' realisation references function '{}' which is not declared",
                                concept.id, function_id
                            )),
                    );
                }
                Some(crate::glossary::TermRealisation::CrossEntity { predicate })
                    if predicate.trim().is_empty() =>
                {
                    errors.push(
                        diag("ontology.validate.concept.empty_cross_entity_predicate")
                            .with("concept_id", concept.id.as_str())
                            .message(format!(
                                "Concept '{}' carries a CrossEntity realisation with an empty predicate",
                                concept.id
                            )),
                    );
                }
                Some(_) => {}
                None => {}
            }
        }

        if let Some(cycle) = find_cycle(self.concepts.iter().map(|c| c.id.clone()), |id| {
            concept_index
                .get(id)
                .and_then(|concept| concept.broader.clone())
                .map(|next| vec![next])
                .unwrap_or_default()
        }) {
            errors.push(
                diag("ontology.validate.concept.broader_cycle")
                    .with(
                        "cycle",
                        cycle
                            .iter()
                            .map(|id| id.as_str())
                            .collect::<Vec<_>>()
                            .join(" → "),
                    )
                    .message(format!(
                        "Concept hierarchy forms a cycle: {}",
                        cycle
                            .iter()
                            .map(|id| id.as_str())
                            .collect::<Vec<_>>()
                            .join(" → ")
                    )),
            );
        }

        if let Some(cycle) = find_cycle(self.concepts.iter().map(|c| c.id.clone()), |id| {
            concept_index
                .get(id)
                .and_then(|concept| concept.replaced_by.clone())
                .map(|next| vec![next])
                .unwrap_or_default()
        }) {
            errors.push(
                diag("ontology.validate.concept.replaced_by_cycle")
                    .with(
                        "cycle",
                        cycle
                            .iter()
                            .map(|id| id.as_str())
                            .collect::<Vec<_>>()
                            .join(" → "),
                    )
                    .message(format!(
                        "Concept replacement chain forms a cycle: {}",
                        cycle
                            .iter()
                            .map(|id| id.as_str())
                            .collect::<Vec<_>>()
                            .join(" → ")
                    )),
            );
        }

        if let Some(cycle) = find_cycle(self.glossary.iter().map(|t| t.id.clone()), |id| {
            glossary_index
                .get(id)
                .map(|term| {
                    term.related_terms
                        .iter()
                        .filter(|r| matches!(r.kind, TermRelationKind::Broader))
                        .map(|r| r.target.clone())
                        .collect()
                })
                .unwrap_or_default()
        }) {
            errors.push(
                diag("ontology.validate.glossary.broader_cycle")
                    .with(
                        "cycle",
                        cycle
                            .iter()
                            .map(|id| id.as_str())
                            .collect::<Vec<_>>()
                            .join(" → "),
                    )
                    .message(format!(
                        "Glossary Broader hierarchy forms a cycle: {}",
                        cycle
                            .iter()
                            .map(|id| id.as_str())
                            .collect::<Vec<_>>()
                            .join(" → ")
                    )),
            );
        }

        if let Some(cycle) = find_cycle(self.glossary.iter().map(|t| t.id.clone()), |id| {
            glossary_index
                .get(id)
                .and_then(|term| match &term.lifecycle {
                    TermLifecycle::Deprecated { replaced_by, .. } => replaced_by.clone(),
                    _ => None,
                })
                .map(|next| vec![next])
                .unwrap_or_default()
        }) {
            errors.push(
                diag("ontology.validate.glossary.replaced_by_cycle")
                    .with(
                        "cycle",
                        cycle
                            .iter()
                            .map(|id| id.as_str())
                            .collect::<Vec<_>>()
                            .join(" → "),
                    )
                    .message(format!(
                        "Glossary deprecation chain forms a cycle: {}",
                        cycle
                            .iter()
                            .map(|id| id.as_str())
                            .collect::<Vec<_>>()
                            .join(" → ")
                    )),
            );
        }

        for rule in &self.rules {
            if let (Some(from), Some(to)) = (rule.valid_from, rule.valid_to)
                && from >= to
            {
                errors.push(
                    diag("ontology.validate.rule.invalid_validity_window")
                        .with("rule_id", rule.id.as_str())
                        .with("valid_from", from.to_rfc3339())
                        .with("valid_to", to.to_rfc3339())
                        .message(format!(
                            "Rule '{}' valid_from ({}) is not strictly before valid_to ({})",
                            rule.id,
                            from.to_rfc3339(),
                            to.to_rfc3339()
                        )),
                );
            }
        }

        for node in &self.node_types {
            for property in &node.properties {
                let mut counts: std::collections::HashMap<&'static str, usize> =
                    std::collections::HashMap::new();
                for binding in &property.bindings {
                    let kind = match binding {
                        PropertyBinding::Concept { .. } => "concept",
                        PropertyBinding::NotationPattern { .. } => "notation_pattern",
                        PropertyBinding::ValueRange { .. } => "value_range",
                        PropertyBinding::ValueSet { .. } | PropertyBinding::CodeSystem { .. } => {
                            continue;
                        }
                    };
                    *counts.entry(kind).or_default() += 1;
                }
                for (kind, count) in counts.into_iter().filter(|(_, n)| *n > 1) {
                    errors.push(
                        diag("ontology.validate.property.duplicate_binding_kind")
                            .with("node_label", node.label.as_str())
                            .with("property_name", property.name.as_str())
                            .with("kind", kind)
                            .with("count", count.to_string())
                            .message(format!(
                                "Property '{}.{}' has {} '{}' bindings — at most one is allowed",
                                node.label.as_str(),
                                property.name.as_str(),
                                count,
                                kind
                            )),
                    );
                }
            }
        }

        let known_concepts: std::collections::HashSet<&str> =
            self.concepts.iter().map(|c| c.id.as_str()).collect();
        for node in &self.node_types {
            if let Some(concept_id) = &node.concept_id
                && !known_concepts.contains(concept_id.as_str())
            {
                errors.push(
                    diag("ontology.validate.node.unknown_concept")
                        .with("node_id", node.id.as_str())
                        .with("concept_id", concept_id.as_str())
                        .message(format!(
                            "Node type '{}' realises concept '{}' which is not declared",
                            node.label.as_str(),
                            concept_id
                        )),
                );
            }
            let mut seen_realizations = std::collections::HashSet::new();
            if let Some(concept_id) = &node.concept_id {
                seen_realizations.insert(concept_id.as_str());
            }
            for realization in &node.concept_realizations {
                if !known_concepts.contains(realization.concept_id.as_str()) {
                    errors.push(
                        diag("ontology.validate.node.unknown_concept_realization")
                            .with("node_id", node.id.as_str())
                            .with("concept_id", realization.concept_id.as_str())
                            .message(format!(
                                "Node type '{}' realises additional concept '{}' which is not declared",
                                node.label.as_str(),
                                realization.concept_id
                            )),
                    );
                }
                if !seen_realizations.insert(realization.concept_id.as_str()) {
                    errors.push(
                        diag("ontology.validate.node.duplicate_concept_realization")
                            .with("node_id", node.id.as_str())
                            .with("concept_id", realization.concept_id.as_str())
                            .message(format!(
                                "Node type '{}' declares duplicate concept realization '{}'",
                                node.label.as_str(),
                                realization.concept_id
                            )),
                    );
                }
            }
        }
        for edge in &self.edge_types {
            if let Some(concept_id) = &edge.concept_id
                && !known_concepts.contains(concept_id.as_str())
            {
                errors.push(
                    diag("ontology.validate.edge.unknown_concept")
                        .with("edge_id", edge.id.as_str())
                        .with("concept_id", concept_id.as_str())
                        .message(format!(
                            "Edge type '{}' realises concept '{}' which is not declared",
                            edge.label.as_str(),
                            concept_id
                        )),
                );
            }
            let mut seen_realizations = std::collections::HashSet::new();
            if let Some(concept_id) = &edge.concept_id {
                seen_realizations.insert(concept_id.as_str());
            }
            for realization in &edge.concept_realizations {
                if !known_concepts.contains(realization.concept_id.as_str()) {
                    errors.push(
                        diag("ontology.validate.edge.unknown_concept_realization")
                            .with("edge_id", edge.id.as_str())
                            .with("concept_id", realization.concept_id.as_str())
                            .message(format!(
                                "Edge type '{}' realises additional concept '{}' which is not declared",
                                edge.label.as_str(),
                                realization.concept_id
                            )),
                    );
                }
                if !seen_realizations.insert(realization.concept_id.as_str()) {
                    errors.push(
                        diag("ontology.validate.edge.duplicate_concept_realization")
                            .with("edge_id", edge.id.as_str())
                            .with("concept_id", realization.concept_id.as_str())
                            .message(format!(
                                "Edge type '{}' declares duplicate concept realization '{}'",
                                edge.label.as_str(),
                                realization.concept_id
                            )),
                    );
                }
            }
        }

        // -------------------------------------------------------------
        // Segment referential integrity. The target NodeType must
        // resolve, and every property the filter mentions must
        // exist on that target — drift here breaks the glossary
        // realisation chain and the runtime's segment-aware
        // compile pass.
        for seg in &self.segments {
            let target_node = self
                .node_types
                .iter()
                .find(|n| n.id == seg.target_node_type_id);
            let Some(target) = target_node else {
                errors.push(
                    diag("ontology.validate.segment.unknown_target_node_type")
                        .with("segment_id", seg.id.as_str())
                        .with("target_node_type_id", seg.target_node_type_id.as_str())
                        .message(format!(
                            "Segment '{}' targets node type '{}' which is not declared on the ontology",
                            seg.id, seg.target_node_type_id
                        )),
                );
                continue;
            };
            let known_properties: std::collections::HashSet<&str> =
                target.properties.iter().map(|p| p.name.as_str()).collect();
            for prop in seg.referenced_properties() {
                if !known_properties.contains(prop.as_str()) {
                    errors.push(
                        diag("ontology.validate.segment.unknown_property")
                            .with("segment_id", seg.id.as_str())
                            .with("target_node_label", target.label.as_str())
                            .with("property", prop.as_str())
                            .message(format!(
                                "Segment '{}' filter references property '{}' which is not declared on \
                                 node type '{}'",
                                seg.id, prop, target.label
                            )),
                    );
                }
            }
        }

        errors
    }

    /// Single-pass referential check for one [`PropertyBinding`].
    /// Returns `Some(diagnostic)` when the binding's target id is
    /// missing from the corresponding registry index. Strength-aware
    /// rejection is layered on top by callers that care.
    fn check_binding_target_exists(
        &self,
        node_label: &str,
        property_name: &str,
        binding: &crate::binding::PropertyBinding,
    ) -> Option<DiagnosticMessage> {
        use crate::binding::PropertyBinding;
        let (registry, missing_id) = match binding {
            PropertyBinding::ValueSet { id, .. } => {
                if self.lookup.value_set_id_idx.contains_key(id) {
                    return None;
                }
                ("value set", id.as_str().to_string())
            }
            PropertyBinding::CodeSystem { id, .. } => {
                if self.lookup.code_system_id_idx.contains_key(id) {
                    return None;
                }
                ("code system", id.as_str().to_string())
            }
            PropertyBinding::NotationPattern { id, .. } => {
                if self.lookup.notation_pattern_id_idx.contains_key(id) {
                    return None;
                }
                ("notation pattern", id.as_str().to_string())
            }
            PropertyBinding::ValueRange { id, .. } => {
                if self.lookup.value_range_set_id_idx.contains_key(id) {
                    return None;
                }
                ("value range set", id.as_str().to_string())
            }
            PropertyBinding::Concept { id, .. } => {
                if self.lookup.concept_id_idx.contains_key(id) {
                    return None;
                }
                ("concept", id.as_str().to_string())
            }
        };
        Some(
            diag("ontology.validate.binding.unknown_target_id")
                .with("node_label", node_label)
                .with("property", property_name)
                .with("registry", registry)
                .with("missing_id", missing_id.clone())
                .message(format!(
                    "Property '{}.{}' binding references unknown {} id '{}'",
                    node_label, property_name, registry, missing_id
                )),
        )
    }

    fn check_binding_concept_map_compatibility(
        &self,
        node_label: &str,
        property_name: &str,
        binding: &crate::binding::PropertyBinding,
    ) -> Option<DiagnosticMessage> {
        use crate::binding::PropertyBinding;

        let concept_map_id = binding.concept_map_id()?;
        let concept_map_idx = *self.lookup.concept_map_id_idx.get(concept_map_id)?;
        let concept_map = &self.concept_maps[concept_map_idx];
        let covers_system = |id: &crate::code_system::CodeSystemId| {
            id == &concept_map.source_system_id || id == &concept_map.target_system_id
        };

        match binding {
            PropertyBinding::CodeSystem { id, .. } => {
                if covers_system(id) {
                    return None;
                }
                Some(
                    diag("ontology.validate.binding.concept_map_system_mismatch")
                        .with("node_label", node_label)
                        .with("property", property_name)
                        .with("binding_kind", "code_system")
                        .with("binding_system_id", id.as_str())
                        .with("concept_map_id", concept_map_id.as_str())
                        .with("source_system_id", concept_map.source_system_id.as_str())
                        .with("target_system_id", concept_map.target_system_id.as_str())
                        .message(format!(
                            "Property '{}.{}' binding references concept map '{}' but code system '{}' is neither its source nor target system",
                            node_label, property_name, concept_map_id, id
                        )),
                )
            }
            PropertyBinding::ValueSet { id, .. } => {
                let value_set_idx = *self.lookup.value_set_id_idx.get(id)?;
                let value_set = &self.value_sets[value_set_idx];
                let uncovered: Vec<&str> = value_set
                    .composition
                    .iter()
                    .filter_map(|rule| {
                        if self.lookup.code_system_id_idx.contains_key(&rule.system_id)
                            && !covers_system(&rule.system_id)
                        {
                            Some(rule.system_id.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                if uncovered.is_empty() {
                    return None;
                }
                Some(
                    diag("ontology.validate.binding.concept_map_system_mismatch")
                        .with("node_label", node_label)
                        .with("property", property_name)
                        .with("binding_kind", "value_set")
                        .with("binding_value_set_id", id.as_str())
                        .with("concept_map_id", concept_map_id.as_str())
                        .with("source_system_id", concept_map.source_system_id.as_str())
                        .with("target_system_id", concept_map.target_system_id.as_str())
                        .with("uncovered_system_ids", uncovered.join(","))
                        .message(format!(
                            "Property '{}.{}' binding references concept map '{}' but value set '{}' includes systems outside that map: {}",
                            node_label,
                            property_name,
                            concept_map_id,
                            id,
                            uncovered.join(", ")
                        )),
                )
            }
            PropertyBinding::NotationPattern { .. }
            | PropertyBinding::ValueRange { .. }
            | PropertyBinding::Concept { .. } => None,
        }
    }

    /// Non-blocking author advisories.
    ///
    /// Where [`validate`](Self::validate) returns rejections that
    /// must be fixed before commit, this returns *guidance* — the
    /// IR is structurally sound but a smaller authoring choice
    /// would reduce drift risk. Today: same-meaning constraints
    /// authored on two surfaces (e.g. `NodeConstraint::Exists` plus
    /// a `ShaclConstraint::MinCount{min:1}` rule on the same
    /// property — single source of truth is preferred).
    ///
    /// Callers surface these in admin UI annotations and as
    /// quality-report rows; they never block writes.
    pub fn advisories(&self) -> Vec<DiagnosticMessage> {
        let mut out = Vec::new();

        // ----- Dedup: NodeConstraint vs ShaclConstraint overlap -----
        //
        // Two surfaces, same intent. Collect the (node, property,
        // intent) triples a node-level structural constraint
        // declares, then walk every property-shape rule looking
        // for a matching SHACL emission.
        use crate::rule::{RuleKind, ShaclConstraint};
        for node in &self.node_types {
            // (property_id → intents this node declares)
            let mut node_intents: std::collections::HashMap<&str, Vec<&'static str>> =
                std::collections::HashMap::new();
            for c in &node.constraints {
                match &c.constraint {
                    crate::ir::NodeConstraint::Exists { property_id } => {
                        node_intents
                            .entry(property_id.as_str())
                            .or_default()
                            .push("required");
                    }
                    crate::ir::NodeConstraint::Unique { property_ids }
                        if property_ids.len() == 1 =>
                    {
                        node_intents
                            .entry(property_ids[0].as_str())
                            .or_default()
                            .push("unique");
                    }
                    crate::ir::NodeConstraint::NodeKey { property_ids }
                        if property_ids.len() == 1 =>
                    {
                        node_intents
                            .entry(property_ids[0].as_str())
                            .or_default()
                            .push("required");
                        node_intents
                            .entry(property_ids[0].as_str())
                            .or_default()
                            .push("unique");
                    }
                    _ => {}
                }
            }

            // For each rule with a property-shape target on this
            // node, see whether its constraint replays an intent
            // already carried by a node-level structural constraint.
            for rule_id in &node.rules {
                let Some(rule) = self.rules.iter().find(|r| r.id == *rule_id) else {
                    continue;
                };
                let RuleKind::PropertyShape {
                    target_node_type_id,
                    target_property_id,
                } = &rule.kind
                else {
                    continue;
                };
                if target_node_type_id != &node.id {
                    continue;
                }
                let Some(intents) = node_intents.get(target_property_id.as_str()) else {
                    continue;
                };
                for sc in &rule.constraints {
                    match sc {
                        ShaclConstraint::MinCount { min: 1, .. }
                            if intents.contains(&"required") =>
                        {
                            out.push(
                                diag("ontology.advisory.duplicate_required_constraint")
                                    .with("node_label", node.label.as_str())
                                    .with("property_id", target_property_id.as_str())
                                    .with("rule_id", rule.id.as_str())
                                    .message(format!(
                                        "Property '{}.{}' is marked Required by both \
                                         a NodeConstraint and rule '{}' (MinCount=1) — \
                                         keep one source of truth",
                                        node.label, target_property_id, rule.id
                                    )),
                            );
                        }
                        ShaclConstraint::UniqueKey { .. } if intents.contains(&"unique") => {
                            out.push(
                                diag("ontology.advisory.duplicate_unique_constraint")
                                    .with("node_label", node.label.as_str())
                                    .with("property_id", target_property_id.as_str())
                                    .with("rule_id", rule.id.as_str())
                                    .message(format!(
                                        "Property '{}.{}' is marked Unique by both a \
                                         NodeConstraint and rule '{}' (UniqueKey) — \
                                         keep one source of truth",
                                        node.label, target_property_id, rule.id
                                    )),
                            );
                        }
                        _ => {}
                    }
                }
            }
        }

        out
    }

    /// `Required` strength promises every write satisfies the
    /// binding's domain. A binding whose target resolves but exposes
    /// an empty domain would reject every write — the IR refuses to
    /// commit that contradiction.
    fn check_required_binding_domain(
        &self,
        node_label: &str,
        property_name: &str,
        binding: &crate::binding::PropertyBinding,
    ) -> Option<DiagnosticMessage> {
        use crate::binding::PropertyBinding;
        match binding {
            PropertyBinding::ValueSet { id, .. } => {
                let vs = self.value_set_by_id(id)?;
                if vs.composition.is_empty() {
                    return Some(
                        diag("ontology.validate.binding.required_value_set_empty")
                            .with("node_label", node_label)
                            .with("property", property_name)
                            .with("value_set_id", id.as_str())
                            .message(format!(
                                "Property '{}.{}' Required binding to value set '{}' is empty — \
                                 no write would be accepted",
                                node_label, property_name, id
                            )),
                    );
                }
                None
            }
            PropertyBinding::CodeSystem { id, .. } => {
                let cs = self.code_system_by_id(id)?;
                if cs.codes.is_empty() {
                    return Some(
                        diag("ontology.validate.binding.required_code_system_empty")
                            .with("node_label", node_label)
                            .with("property", property_name)
                            .with("code_system_id", id.as_str())
                            .message(format!(
                                "Property '{}.{}' Required binding to code system '{}' has no codes — \
                                 no write would be accepted",
                                node_label, property_name, id
                            )),
                    );
                }
                None
            }
            PropertyBinding::NotationPattern { id, .. } => {
                let np = self.notation_pattern_by_id(id)?;
                if np.components.is_empty() {
                    return Some(
                        diag("ontology.validate.binding.required_notation_pattern_empty")
                            .with("node_label", node_label)
                            .with("property", property_name)
                            .with("notation_pattern_id", id.as_str())
                            .message(format!(
                                "Property '{}.{}' Required binding to notation pattern '{}' has no \
                                 components — pattern accepts nothing",
                                node_label, property_name, id
                            )),
                    );
                }
                None
            }
            // ValueRange and Concept variants don't carry a strength
            // field — they cannot be `Required` and are unreachable
            // from the strength()==Required gate above.
            PropertyBinding::ValueRange { .. } | PropertyBinding::Concept { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::binding::PropertyBinding;
    use crate::code_system::{
        CodeSystemDef, CodeSystemId, CodeSystemKind, CodedValue, CodedValueId,
    };
    use crate::concept_map::{ConceptMapDef, ConceptMapId, ConceptMapping, Equivalence};
    use crate::ir::*;
    use crate::value_set::{
        IncludeMode, ValueSetDef, ValueSetId, ValueSetIncludeRule, ValueSetSelector,
    };
    use ox_core::graph_label::GraphLabel;
    use ox_core::i18n::LocalizedText;
    use ox_core::property_key::PropertyKey;
    use ox_core::types::PropertyType;

    // Shared fixture helpers — validation tests and the sibling
    // `ir::tests` module both draw from `test_fixtures` so the
    // starting ontology stays byte-identical across modules.
    // Diverging the two fixtures silently in the past has caused
    // tests to pass on one shape while failing on the other.
    use crate::test_fixtures::{property_nullable as property, sample_user_ontology};

    fn base_ontology() -> OntologyIR {
        sample_user_ontology()
    }

    fn cv(id: &str, code: &str) -> CodedValue {
        CodedValue {
            id: CodedValueId::new(id),
            code: code.into(),
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
        }
    }

    fn code_system(id: &str, codes: Vec<CodedValue>) -> CodeSystemDef {
        CodeSystemDef {
            id: CodeSystemId::new(id),
            name: id.into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            uri: None,
            version: "1".into(),
            kind: CodeSystemKind::Internal,
            hierarchical: false,
            codes,
            deprecated_at: None,
            replaced_by_id: None,
        }
    }

    fn status_ontology(binding: PropertyBinding) -> OntologyIR {
        OntologyIR::try_new(
            "ont".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![NodeTypeDef {
                id: NodeTypeId::new("nt"),
                label: GraphLabel::new("Doc").expect("valid label"),
                properties: vec![PropertyDef {
                    id: PropertyId::new("p-status"),
                    name: PropertyKey::new("status").expect("valid property key"),
                    property_type: PropertyType::String,
                    bindings: vec![binding],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            vec![],
            vec![],
        )
        .expect("valid ontology")
    }

    #[test]
    fn validate_accepts_well_formed_ontology() {
        let ontology = base_ontology();
        assert!(ontology.validate().is_empty());
    }

    #[test]
    fn validate_rejects_code_system_binding_with_incompatible_concept_map() {
        let mut ontology = status_ontology(
            PropertyBinding::code_system(CodeSystemId::new("cs-other"))
                .with_concept_map(ConceptMapId::new("cm-status")),
        );
        ontology
            .add_code_system(code_system("cs-source", vec![cv("cv-active", "ACTIVE")]))
            .expect("source code system");
        ontology
            .add_code_system(code_system("cs-target", vec![cv("cv-a", "A")]))
            .expect("target code system");
        ontology
            .add_code_system(code_system("cs-other", vec![cv("cv-open", "OPEN")]))
            .expect("other code system");
        ontology
            .add_concept_map(ConceptMapDef {
                id: ConceptMapId::new("cm-status"),
                name: "StatusMap".into(),
                display_name: LocalizedText::default(),
                description: LocalizedText::default(),
                version: "1".into(),
                source_system_id: CodeSystemId::new("cs-source"),
                target_system_id: CodeSystemId::new("cs-target"),
                mappings: vec![ConceptMapping {
                    source_code: "ACTIVE".into(),
                    target_code: "A".into(),
                    equivalence: Equivalence::Equivalent,
                    comment: LocalizedText::default(),
                }],
            })
            .expect("concept map");

        let errors = ontology.validate();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.binding.concept_map_system_mismatch"),
            "expected concept-map system mismatch: {errors:?}"
        );
    }

    #[test]
    fn validate_rejects_value_set_binding_with_partly_uncovered_concept_map() {
        let mut ontology = status_ontology(
            PropertyBinding::value_set(ValueSetId::new("vs-status"))
                .with_concept_map(ConceptMapId::new("cm-status")),
        );
        ontology
            .add_code_system(code_system("cs-source", vec![cv("cv-active", "ACTIVE")]))
            .expect("source code system");
        ontology
            .add_code_system(code_system("cs-target", vec![cv("cv-a", "A")]))
            .expect("target code system");
        ontology
            .add_code_system(code_system("cs-other", vec![cv("cv-open", "OPEN")]))
            .expect("other code system");
        ontology
            .add_value_set(ValueSetDef {
                id: ValueSetId::new("vs-status"),
                name: "Status".into(),
                display_name: LocalizedText::default(),
                description: LocalizedText::default(),
                version: "1".into(),
                composition: vec![
                    ValueSetIncludeRule {
                        system_id: CodeSystemId::new("cs-source"),
                        selector: ValueSetSelector::All,
                        mode: IncludeMode::Include,
                    },
                    ValueSetIncludeRule {
                        system_id: CodeSystemId::new("cs-other"),
                        selector: ValueSetSelector::All,
                        mode: IncludeMode::Include,
                    },
                ],
            })
            .expect("value set");
        ontology
            .add_concept_map(ConceptMapDef {
                id: ConceptMapId::new("cm-status"),
                name: "StatusMap".into(),
                display_name: LocalizedText::default(),
                description: LocalizedText::default(),
                version: "1".into(),
                source_system_id: CodeSystemId::new("cs-source"),
                target_system_id: CodeSystemId::new("cs-target"),
                mappings: vec![ConceptMapping {
                    source_code: "ACTIVE".into(),
                    target_code: "A".into(),
                    equivalence: Equivalence::Equivalent,
                    comment: LocalizedText::default(),
                }],
            })
            .expect("concept map");

        let errors = ontology.validate();
        assert!(
            errors.iter().any(|e| {
                e.code == "ontology.validate.binding.concept_map_system_mismatch"
                    && e.params
                        .get("uncovered_system_ids")
                        .map(|v| v == "cs-other")
                        .unwrap_or(false)
            }),
            "expected uncovered value-set system mismatch: {errors:?}"
        );
    }

    #[test]
    fn validate_rejects_duplicate_properties_and_bad_indexes() {
        let mut ontology = base_ontology();
        ontology.node_types[0]
            .properties
            .push(property("prop-email-dup", "email", false));
        ontology.indexes.push(IndexDef::Composite {
            id: "idx-composite".to_string(),
            node_id: "node-user".into(),
            property_ids: vec!["prop-email".into(), "prop-missing".into()],
        });

        let errors = ontology.validate();

        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.property.duplicate_name"),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.index.unknown_property_id"),
            "{errors:?}"
        );
    }

    #[test]
    fn validate_rejects_nullable_required_constraints() {
        let mut ontology = base_ontology();
        ontology.node_types[0].properties[0].nullable = true;

        let errors = ontology.validate();

        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.constraint.requires_non_nullable"),
            "{errors:?}"
        );
    }

    #[test]
    fn validate_rejects_empty_id_name_and_no_node_types() {
        let ontology = OntologyIR::new(
            "  ".to_string(),
            String::new(),
            LocalizedText::default(),
            1,
            vec![],
            vec![],
            vec![],
        );

        let errors = ontology.validate();

        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.id.empty")
        );
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.name.empty")
        );
        // Empty everything also fails the "populate at least one
        // collection" invariant.
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.no_content")
        );
    }

    #[test]
    fn validate_rejects_edge_referencing_unknown_node_id() {
        let mut ontology = base_ontology();
        ontology.edge_types[0].source_node_id = "node-nonexistent".into();

        let errors = ontology.validate();

        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.edge.unknown_source_node_id"),
            "{errors:?}"
        );
    }

    #[test]
    fn validate_rejects_constraint_referencing_unknown_property_id() {
        let mut ontology = base_ontology();
        ontology.node_types[0].constraints.push(ConstraintDef {
            id: "cst-bad".into(),
            constraint: NodeConstraint::Unique {
                property_ids: vec!["prop-nonexistent".into()],
            },
        });

        let errors = ontology.validate();

        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.constraint.unknown_property_id"),
            "{errors:?}"
        );
    }

    #[test]
    fn validate_flags_measure_aggregation_role_on_non_numeric_property() {
        // `status` is typed String but declared Measure — a
        // semantic error: you cannot SUM or AVG a string column.
        let mut ontology = base_ontology();
        ontology.node_types[0].properties[0].aggregation_role = Some(AggregationRole::Measure);
        // The fixture's first property is String-typed; keep it.

        let errors = ontology.validate();

        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.property.measure_non_numeric"),
            "expected Measure/non-numeric warning, got: {errors:?}"
        );
    }

    #[test]
    fn validate_rejects_source_mapped_property_without_binding() {
        let mut ontology = base_ontology();
        // Pick the first property that has a source_column set in
        // the fixture and clear its bindings so we hit the new rule.
        let prop = &mut ontology.node_types[0].properties[0];
        prop.source_column = Some("name_col".to_string());
        prop.bindings.clear();
        prop.binding_exempt = None;
        prop.aggregation_role = None;

        let errors = ontology.validate();

        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.property.mapping_without_binding"),
            "expected mapping_without_binding diagnostic, got: {errors:?}"
        );
    }

    #[test]
    fn validate_accepts_source_mapped_property_with_explicit_exemption() {
        let mut ontology = base_ontology();
        let prop = &mut ontology.node_types[0].properties[0];
        prop.source_column = Some("id_col".to_string());
        prop.bindings.clear();
        prop.binding_exempt = Some(super::super::BindingExemptReason::PrimaryKey);

        let errors = ontology.validate();

        assert!(
            !errors
                .iter()
                .any(|e| e.code == "ontology.validate.property.mapping_without_binding"),
            "explicit exemption must suppress diagnostic: {errors:?}"
        );
    }

    #[test]
    fn validate_accepts_identifier_role_as_implicit_exemption() {
        let mut ontology = base_ontology();
        let prop = &mut ontology.node_types[0].properties[0];
        prop.source_column = Some("id_col".to_string());
        prop.bindings.clear();
        prop.binding_exempt = None;
        prop.aggregation_role = Some(AggregationRole::Identifier);

        let errors = ontology.validate();

        assert!(
            !errors
                .iter()
                .any(|e| e.code == "ontology.validate.property.mapping_without_binding"),
            "Identifier role must be implicit exemption: {errors:?}"
        );
    }

    #[test]
    fn validate_rejects_composition_with_many_to_many_cardinality() {
        let mut ontology = base_ontology();
        // Promote first edge to Composition while leaving its
        // cardinality at the default ManyToMany — should reject.
        ontology.edge_types[0].kind = EdgeKind::Composition;
        ontology.edge_types[0].cardinality = Cardinality::ManyToMany;

        let errors = ontology.validate();

        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.edge.composition_requires_singular_source"),
            "expected composition cardinality diagnostic: {errors:?}"
        );
    }

    #[test]
    fn validate_accepts_composition_with_one_to_many_cardinality() {
        let mut ontology = base_ontology();
        ontology.edge_types[0].kind = EdgeKind::Composition;
        ontology.edge_types[0].cardinality = Cardinality::OneToMany;

        let errors = ontology.validate();

        assert!(
            !errors
                .iter()
                .any(|e| e.code == "ontology.validate.edge.composition_requires_singular_source"),
            "OneToMany composition is the canonical case: {errors:?}"
        );
    }

    #[test]
    fn validate_accepts_segment_targeting_real_node_with_real_properties() {
        use crate::segment::{SegmentDef, SegmentFilter, SegmentLiteral};
        use ox_core::PropertyKey;

        let mut ontology = base_ontology();
        let target_node_id = ontology.node_types[0].id.clone();
        let target_property = ontology.node_types[0].properties[0]
            .name
            .as_str()
            .to_string();

        ontology
            .add_segment(SegmentDef {
                id: "seg-active".into(),
                name: "active".to_string(),
                display_name: LocalizedText::default(),
                description: LocalizedText::default(),
                target_node_type_id: target_node_id,
                filter: SegmentFilter::Equals {
                    property: PropertyKey::new(target_property)
                        .expect("test property name is valid"),
                    value: SegmentLiteral::Bool { value: true },
                },
                overlap_policy: Default::default(),
                refresh_policy: Default::default(),
            })
            .expect("add segment");

        let errors = ontology.validate();
        assert!(
            !errors
                .iter()
                .any(|e| e.code.starts_with("ontology.validate.segment.")),
            "well-formed segment must validate: {errors:?}"
        );
    }

    #[test]
    fn validate_rejects_segment_targeting_unknown_node_type() {
        use crate::segment::{SegmentDef, SegmentFilter, SegmentLiteral};
        use ox_core::PropertyKey;

        let mut ontology = base_ontology();
        ontology
            .add_segment(SegmentDef {
                id: "seg-bad".into(),
                name: "bad".to_string(),
                display_name: LocalizedText::default(),
                description: LocalizedText::default(),
                target_node_type_id: "nt-does-not-exist".into(),
                filter: SegmentFilter::Equals {
                    property: PropertyKey::new("anything").unwrap(),
                    value: SegmentLiteral::Bool { value: true },
                },
                overlap_policy: Default::default(),
                refresh_policy: Default::default(),
            })
            .expect("add survives — referential check is in validate()");

        let errors = ontology.validate();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.segment.unknown_target_node_type"),
            "expected unknown-target-node diagnostic: {errors:?}"
        );
    }

    #[test]
    fn validate_rejects_segment_referencing_unknown_property() {
        use crate::segment::{SegmentDef, SegmentFilter, SegmentLiteral};
        use ox_core::PropertyKey;

        let mut ontology = base_ontology();
        let target_node_id = ontology.node_types[0].id.clone();

        ontology
            .add_segment(SegmentDef {
                id: "seg-prop-missing".into(),
                name: "prop_missing".to_string(),
                display_name: LocalizedText::default(),
                description: LocalizedText::default(),
                target_node_type_id: target_node_id,
                filter: SegmentFilter::Equals {
                    property: PropertyKey::new("never_declared_property").unwrap(),
                    value: SegmentLiteral::Int { value: 1 },
                },
                overlap_policy: Default::default(),
                refresh_policy: Default::default(),
            })
            .expect("add segment");

        let errors = ontology.validate();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.segment.unknown_property"),
            "expected unknown-property diagnostic: {errors:?}"
        );
    }

    #[test]
    fn add_segment_rejects_duplicate_id() {
        use crate::segment::{SegmentDef, SegmentFilter, SegmentLiteral};
        use ox_core::PropertyKey;

        let mut ontology = base_ontology();
        let target_node_id = ontology.node_types[0].id.clone();
        let make = || SegmentDef {
            id: "seg-dup".into(),
            name: "dup".to_string(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            target_node_type_id: target_node_id.clone(),
            filter: SegmentFilter::Equals {
                property: PropertyKey::new("anything").unwrap(),
                value: SegmentLiteral::Bool { value: true },
            },
            overlap_policy: Default::default(),
            refresh_policy: Default::default(),
        };

        ontology.add_segment(make()).expect("first insert");
        let err = ontology
            .add_segment(make())
            .expect_err("duplicate id must reject");
        match err {
            crate::ir::OntologyInvariantError::DuplicateCollectionId { kind, .. } => {
                assert_eq!(kind, "segment");
            }
            other => panic!("expected DuplicateCollectionId, got {other:?}"),
        }
    }

    #[test]
    fn validate_accepts_inventory_pointing_at_declared_node() {
        let mut ontology = base_ontology();
        let target_node_id = ontology.node_types[0].id.clone();
        ontology
            .upsert_table_inventory_entry(crate::table_inventory::TableInventoryEntry::imported(
                crate::mapping::SourceId::new("pg-main"),
                "users",
                "fp-1",
                vec![target_node_id],
            ))
            .expect("upsert");
        let errors = ontology.validate();
        assert!(
            !errors
                .iter()
                .any(|e| e.code.starts_with("ontology.validate.table_inventory.")),
            "well-formed inventory must validate: {errors:?}"
        );
    }

    #[test]
    fn validate_rejects_inventory_pointing_at_unknown_node() {
        let mut ontology = base_ontology();
        ontology
            .upsert_table_inventory_entry(crate::table_inventory::TableInventoryEntry::imported(
                crate::mapping::SourceId::new("pg-main"),
                "users",
                "fp-1",
                vec![NodeTypeId::new("nt-not-declared")],
            ))
            .expect("upsert");
        let errors = ontology.validate();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.table_inventory.unknown_node_type"),
            "expected unknown_node_type diagnostic: {errors:?}"
        );
    }

    #[test]
    fn upsert_table_inventory_entry_replaces_on_natural_key() {
        let mut ontology = base_ontology();
        let nid = ontology.node_types[0].id.clone();
        ontology
            .upsert_table_inventory_entry(crate::table_inventory::TableInventoryEntry::imported(
                crate::mapping::SourceId::new("pg-main"),
                "users",
                "fp-1",
                vec![nid.clone()],
            ))
            .expect("first upsert");
        ontology
            .upsert_table_inventory_entry(crate::table_inventory::TableInventoryEntry::imported(
                crate::mapping::SourceId::new("pg-main"),
                "users",
                "fp-2",
                vec![nid.clone()],
            ))
            .expect("second upsert");
        assert_eq!(
            ontology.table_inventory().len(),
            1,
            "natural-key collision must replace, not append"
        );
        assert_eq!(ontology.table_inventory()[0].schema_fingerprint, "fp-2");
    }

    #[test]
    fn find_table_inventory_entry_resolves_in_constant_time() {
        let mut ontology = base_ontology();
        let nid = ontology.node_types[0].id.clone();
        ontology
            .upsert_table_inventory_entry(crate::table_inventory::TableInventoryEntry::imported(
                crate::mapping::SourceId::new("pg-main"),
                "users",
                "fp-1",
                vec![nid],
            ))
            .expect("upsert");
        let resolved =
            ontology.find_table_inventory_entry(&crate::mapping::SourceId::new("pg-main"), "users");
        assert!(resolved.is_some());
        assert!(
            ontology
                .find_table_inventory_entry(&crate::mapping::SourceId::new("pg-main"), "missing",)
                .is_none()
        );
    }

    // Concept-term referential integrity (terms with realisation).

    fn term(id: &str, name: &str) -> crate::glossary::GlossaryTermDef {
        crate::glossary::GlossaryTermDef {
            id: crate::glossary::GlossaryTermId::new(id),
            term: LocalizedText::new(name),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            aliases: Vec::new(),
            related_terms: Vec::new(),
            governance: crate::glossary::TermGovernance::default(),
            valid_from: None,
            valid_to: None,
            lifecycle: crate::glossary::TermLifecycle::default(),
            concept_id: None,
            term_pos: Default::default(),
        }
    }

    fn term_for_concept(
        id: &str,
        name: &str,
        concept_id: &str,
    ) -> crate::glossary::GlossaryTermDef {
        let mut term = term(id, name);
        term.concept_id = Some(crate::concept::ConceptId::new(concept_id));
        term
    }

    fn concept_with_realisation(
        id: &str,
        canonical_term: &str,
        realisation: crate::glossary::TermRealisation,
    ) -> crate::concept::ConceptDef {
        crate::concept::ConceptDef {
            id: crate::concept::ConceptId::new(id),
            canonical_term_id: crate::glossary::GlossaryTermId::new(canonical_term),
            alias_term_ids: Vec::new(),
            broader: None,
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            realisation: Some(realisation),
            lifecycle: crate::glossary::TermLifecycle::default(),
            replaced_by: None,
            valid_from: None,
            valid_to: None,
            governance: crate::concept::ConceptGovernance::default(),
        }
    }

    fn concept(id: &str, canonical_term: &str) -> crate::concept::ConceptDef {
        crate::concept::ConceptDef {
            id: crate::concept::ConceptId::new(id),
            canonical_term_id: crate::glossary::GlossaryTermId::new(canonical_term),
            alias_term_ids: Vec::new(),
            broader: None,
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            realisation: None,
            lifecycle: crate::glossary::TermLifecycle::default(),
            replaced_by: None,
            valid_from: None,
            valid_to: None,
            governance: crate::concept::ConceptGovernance::default(),
        }
    }

    #[test]
    fn validate_accepts_term_without_realisation() {
        let mut ontology = base_ontology();
        ontology.glossary.push(term("gt-customer", "Customer"));

        let errors = ontology.validate();
        assert!(
            !errors
                .iter()
                .any(|e| e.code.starts_with("ontology.validate.glossary_term.")),
            "plain glossary term must validate: {errors:?}"
        );
    }

    #[test]
    fn validate_rejects_concept_realisation_pointing_at_missing_segment() {
        let mut ontology = base_ontology();
        ontology
            .glossary
            .push(term_for_concept("gt-active", "Active", "c-active"));
        ontology
            .add_concept(concept_with_realisation(
                "c-active",
                "gt-active",
                crate::glossary::TermRealisation::Segment {
                    segment_id: crate::segment::SegmentId::new("seg-missing"),
                },
            ))
            .expect("declare concept");

        let errors = ontology.validate();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.concept.unknown_realisation_segment"),
            "expected unknown_realisation_segment diagnostic: {errors:?}"
        );
    }

    #[test]
    fn validate_rejects_node_back_reference_to_unknown_term() {
        let mut ontology = base_ontology();
        ontology.node_types[0].concept_id = Some(crate::concept::ConceptId::new("c-not-declared"));

        let errors = ontology.validate();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.node.unknown_concept"),
            "expected node.unknown_concept diagnostic: {errors:?}"
        );
    }

    #[test]
    fn reverse_index_resolves_implementing_node_types_in_constant_time() {
        let mut ontology = base_ontology();
        ontology
            .glossary
            .push(term_for_concept("gt-customer", "Customer", "c-customer"));
        ontology.node_types[0].concept_id = Some(crate::concept::ConceptId::new("c-customer"));
        ontology.rebuild_indices().expect("rebuild");

        let implementers =
            ontology.node_types_realising_concept(&crate::concept::ConceptId::new("c-customer"));
        assert_eq!(implementers.len(), 1);
        assert_eq!(implementers[0].id, ontology.node_types[0].id);
    }

    #[test]
    fn multiple_node_types_may_share_one_concept_term() {
        // CRM.Customer + ERP.Customer both realising the workspace's
        // Customer concept is the canonical multi-source case — the
        // shared `concept_id` is what the federation planner
        // walks to enumerate implementers.
        let mut ontology = base_ontology();
        ontology
            .glossary
            .push(term_for_concept("gt-customer", "Customer", "c-customer"));
        ontology
            .add_concept(crate::concept::ConceptDef {
                id: crate::concept::ConceptId::new("c-customer"),
                canonical_term_id: crate::glossary::GlossaryTermId::new("gt-customer"),
                alias_term_ids: Vec::new(),
                broader: None,
                description: LocalizedText::default(),
                examples: Vec::new(),
                category: None,
                realisation: None,
                lifecycle: crate::glossary::TermLifecycle::default(),
                replaced_by: None,
                valid_from: None,
                valid_to: None,
                governance: crate::concept::ConceptGovernance::default(),
            })
            .expect("declare concept");
        ontology.node_types[0].concept_id = Some(crate::concept::ConceptId::new("c-customer"));

        let extra = NodeTypeDef {
            id: "nt-erp-customer".into(),
            label: GraphLabel::new("ErpCustomer").unwrap(),
            description: LocalizedText::default(),
            properties: Vec::new(),
            constraints: Vec::new(),
            concept_id: Some(crate::concept::ConceptId::new("c-customer")),
            ..Default::default()
        };
        ontology
            .add_node_type(extra)
            .expect("add second implementer");

        let errors = ontology.validate();
        assert!(
            !errors
                .iter()
                .any(|e| e.code.starts_with("ontology.validate.node.")),
            "shared concept term across implementers is the supported pattern: {errors:?}"
        );

        let implementers =
            ontology.node_types_realising_concept(&crate::concept::ConceptId::new("c-customer"));
        assert_eq!(implementers.len(), 2);
    }

    #[test]
    fn validate_rejects_concept_canonical_term_pointing_at_missing_glossary_entry() {
        // Validator runs over arbitrary IRs — including ones
        // loaded from JSON storage where `add_concept`'s
        // referential check didn't run. Drop directly into the
        // backing Vec to simulate that path; the validator is
        // the safety net.
        let mut ontology = base_ontology();
        ontology.glossary.push(term("gt-customer", "Customer"));
        ontology.concepts.push(crate::concept::ConceptDef {
            id: crate::concept::ConceptId::new("c-customer"),
            canonical_term_id: crate::glossary::GlossaryTermId::new("gt-missing"),
            alias_term_ids: Vec::new(),
            broader: None,
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            realisation: None,
            lifecycle: crate::glossary::TermLifecycle::default(),
            replaced_by: None,
            valid_from: None,
            valid_to: None,
            governance: crate::concept::ConceptGovernance::default(),
        });

        let errors = ontology.validate();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.concept.unknown_canonical_term"),
            "expected unknown_canonical_term diagnostic: {errors:?}",
        );
    }

    #[test]
    fn validate_rejects_glossary_term_pointing_at_missing_concept() {
        let mut ontology = base_ontology();
        ontology
            .glossary
            .push(term_for_concept("gt-customer", "Customer", "c-missing"));

        let errors = ontology.validate();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.glossary_term.unknown_concept"),
            "expected glossary_term.unknown_concept diagnostic: {errors:?}",
        );
    }

    #[test]
    fn validate_rejects_concept_canonical_term_without_back_reference() {
        let mut ontology = base_ontology();
        ontology.glossary.push(term("gt-customer", "Customer"));
        ontology.concepts.push(crate::concept::ConceptDef {
            id: crate::concept::ConceptId::new("c-customer"),
            canonical_term_id: crate::glossary::GlossaryTermId::new("gt-customer"),
            alias_term_ids: Vec::new(),
            broader: None,
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            realisation: None,
            lifecycle: crate::glossary::TermLifecycle::default(),
            replaced_by: None,
            valid_from: None,
            valid_to: None,
            governance: crate::concept::ConceptGovernance::default(),
        });

        let errors = ontology.validate();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.concept.canonical_term_concept_mismatch"),
            "expected canonical_term_concept_mismatch diagnostic: {errors:?}",
        );
    }

    #[test]
    fn validate_rejects_concept_alias_pointing_at_missing_glossary_entry() {
        let mut ontology = base_ontology();
        ontology
            .glossary
            .push(term_for_concept("gt-customer", "Customer", "c-customer"));
        ontology.concepts.push(crate::concept::ConceptDef {
            id: crate::concept::ConceptId::new("c-customer"),
            canonical_term_id: crate::glossary::GlossaryTermId::new("gt-customer"),
            alias_term_ids: vec![crate::glossary::GlossaryTermId::new("gt-missing-alias")],
            broader: None,
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            realisation: None,
            lifecycle: crate::glossary::TermLifecycle::default(),
            replaced_by: None,
            valid_from: None,
            valid_to: None,
            governance: crate::concept::ConceptGovernance::default(),
        });

        let errors = ontology.validate();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.concept.unknown_alias_term"),
            "expected unknown_alias_term diagnostic: {errors:?}",
        );
    }

    #[test]
    fn validate_rejects_concept_alias_term_with_wrong_back_reference() {
        let mut ontology = base_ontology();
        ontology
            .glossary
            .push(term_for_concept("gt-customer", "Customer", "c-customer"));
        ontology
            .glossary
            .push(term_for_concept("gt-client", "Client", "c-client"));
        ontology.concepts.push(crate::concept::ConceptDef {
            id: crate::concept::ConceptId::new("c-customer"),
            canonical_term_id: crate::glossary::GlossaryTermId::new("gt-customer"),
            alias_term_ids: vec![crate::glossary::GlossaryTermId::new("gt-client")],
            broader: None,
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            realisation: None,
            lifecycle: crate::glossary::TermLifecycle::default(),
            replaced_by: None,
            valid_from: None,
            valid_to: None,
            governance: crate::concept::ConceptGovernance::default(),
        });

        let errors = ontology.validate();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.concept.alias_term_concept_mismatch"),
            "expected alias_term_concept_mismatch diagnostic: {errors:?}",
        );
    }

    #[test]
    fn validate_rejects_duplicate_concept_lexical_terms() {
        let mut ontology = base_ontology();
        ontology
            .glossary
            .push(term_for_concept("gt-customer", "Customer", "c-customer"));
        ontology.concepts.push(crate::concept::ConceptDef {
            id: crate::concept::ConceptId::new("c-customer"),
            canonical_term_id: crate::glossary::GlossaryTermId::new("gt-customer"),
            alias_term_ids: vec![crate::glossary::GlossaryTermId::new("gt-customer")],
            broader: None,
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            realisation: None,
            lifecycle: crate::glossary::TermLifecycle::default(),
            replaced_by: None,
            valid_from: None,
            valid_to: None,
            governance: crate::concept::ConceptGovernance::default(),
        });

        let errors = ontology.validate();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.concept.duplicate_lexical_term"),
            "expected duplicate_lexical_term diagnostic: {errors:?}",
        );
    }

    #[test]
    fn validate_rejects_glossary_term_reused_by_multiple_concepts() {
        let mut ontology = base_ontology();
        ontology
            .glossary
            .push(term_for_concept("gt-customer", "Customer", "c-customer"));
        for id in ["c-customer", "c-client"] {
            ontology.concepts.push(crate::concept::ConceptDef {
                id: crate::concept::ConceptId::new(id),
                canonical_term_id: crate::glossary::GlossaryTermId::new("gt-customer"),
                alias_term_ids: Vec::new(),
                broader: None,
                description: LocalizedText::default(),
                examples: Vec::new(),
                category: None,
                realisation: None,
                lifecycle: crate::glossary::TermLifecycle::default(),
                replaced_by: None,
                valid_from: None,
                valid_to: None,
                governance: crate::concept::ConceptGovernance::default(),
            });
        }

        let errors = ontology.validate();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.concept.term_reused"),
            "expected term_reused diagnostic: {errors:?}",
        );
    }

    #[test]
    fn validate_rejects_concept_broader_pointing_at_missing_concept() {
        let mut ontology = base_ontology();
        ontology.glossary.push(term("gt-customer", "Customer"));
        ontology.concepts.push(crate::concept::ConceptDef {
            id: crate::concept::ConceptId::new("c-customer"),
            canonical_term_id: crate::glossary::GlossaryTermId::new("gt-customer"),
            alias_term_ids: Vec::new(),
            broader: Some(crate::concept::ConceptId::new("c-party")),
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            realisation: None,
            lifecycle: crate::glossary::TermLifecycle::default(),
            replaced_by: None,
            valid_from: None,
            valid_to: None,
            governance: crate::concept::ConceptGovernance::default(),
        });

        let errors = ontology.validate();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.concept.unknown_broader"),
            "expected unknown_broader diagnostic: {errors:?}",
        );
    }

    #[test]
    fn validate_rejects_concept_broader_cycle() {
        let mut ontology = base_ontology();
        ontology
            .glossary
            .push(term_for_concept("gt-party", "Party", "c-party"));
        ontology
            .glossary
            .push(term_for_concept("gt-customer", "Customer", "c-customer"));
        let mut party = concept("c-party", "gt-party");
        party.broader = Some(crate::concept::ConceptId::new("c-customer"));
        let mut customer = concept("c-customer", "gt-customer");
        customer.broader = Some(crate::concept::ConceptId::new("c-party"));
        ontology.concepts.push(party);
        ontology.concepts.push(customer);

        let errors = ontology.validate();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.concept.broader_cycle"),
            "expected concept.broader_cycle diagnostic: {errors:?}",
        );
    }

    #[test]
    fn validate_rejects_concept_replaced_by_cycle() {
        let mut ontology = base_ontology();
        ontology.glossary.push(term_for_concept(
            "gt-customer-old",
            "Customer old",
            "c-customer-old",
        ));
        ontology.glossary.push(term_for_concept(
            "gt-customer-new",
            "Customer new",
            "c-customer-new",
        ));
        let mut old = concept("c-customer-old", "gt-customer-old");
        old.replaced_by = Some(crate::concept::ConceptId::new("c-customer-new"));
        let mut new = concept("c-customer-new", "gt-customer-new");
        new.replaced_by = Some(crate::concept::ConceptId::new("c-customer-old"));
        ontology.concepts.push(old);
        ontology.concepts.push(new);

        let errors = ontology.validate();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.concept.replaced_by_cycle"),
            "expected concept.replaced_by_cycle diagnostic: {errors:?}",
        );
    }

    #[test]
    fn validate_rejects_concept_replaced_by_pointing_at_missing_concept() {
        let mut ontology = base_ontology();
        ontology
            .glossary
            .push(term_for_concept("gt-customer", "Customer", "c-customer"));
        let mut customer = concept("c-customer", "gt-customer");
        customer.replaced_by = Some(crate::concept::ConceptId::new("c-missing"));
        ontology.concepts.push(customer);

        let errors = ontology.validate();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.concept.unknown_replaced_by"),
            "expected concept.unknown_replaced_by diagnostic: {errors:?}",
        );
    }

    #[test]
    fn validate_rejects_concept_with_inverted_validity_window() {
        let mut ontology = base_ontology();
        ontology
            .glossary
            .push(term_for_concept("gt-customer", "Customer", "c-customer"));
        let now = chrono::Utc::now();
        let mut customer = concept("c-customer", "gt-customer");
        customer.valid_from = Some(now);
        customer.valid_to = Some(now);
        ontology.concepts.push(customer);

        let errors = ontology.validate();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.concept.invalid_validity_window"),
            "expected concept.invalid_validity_window diagnostic: {errors:?}",
        );
    }

    #[test]
    fn validate_rejects_derived_rule_with_missing_source_binding() {
        use crate::rule::{RuleDef, RuleKind, RuleOrigin};

        let mut ontology = base_ontology();
        // Strip every binding from the first property so the derived
        // rule's pointer is dangling.
        let target_node_id = ontology.node_types[0].id.clone();
        let target_property_id = ontology.node_types[0].properties[0].id.clone();
        ontology.node_types[0].properties[0].bindings.clear();

        ontology.rules.push(RuleDef {
            id: "rule-derived".into(),
            name: LocalizedText::new("derived"),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::PropertyShape {
                target_node_type_id: target_node_id.clone(),
                target_property_id: target_property_id.clone(),
            },
            severity: Default::default(),
            enforcement: Default::default(),
            activation: Default::default(),
            origin: RuleOrigin::DerivedFromBinding {
                node_type_id: target_node_id,
                property_id: target_property_id,
            },
            constraints: Vec::new(),
            valid_from: None,
            valid_to: None,
            sh_message: None,
        });
        ontology.rebuild_indices().expect("rebuild");

        let errors = ontology.validate();

        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.rule.derived_origin_missing_binding"),
            "expected derived-rule orphan diagnostic: {errors:?}"
        );
    }

    #[test]
    fn validate_rejects_object_mapping_with_unknown_property_mapping() {
        use crate::mapping::{ColumnRef, ObjectMappingDef, PropertyLocation, PropertyMappingDef};

        let mut ontology = base_ontology();
        let mut mapping = ObjectMappingDef::new("om-user", "node-user", "pg-main", "users");
        mapping.property_mappings.push(PropertyMappingDef {
            property_id: "prop-missing".into(),
            property_key: PropertyKey::new("missing").expect("valid property key"),
            location: PropertyLocation::Column(ColumnRef::new("users", "missing")),
            transform: Default::default(),
            concept_map_id: None,
        });
        ontology.object_mappings.push(mapping);
        ontology.rebuild_indices().expect("rebuild");

        let errors = ontology.validate();

        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.object_mapping.unknown_property_id"),
            "expected object mapping unknown property diagnostic: {errors:?}"
        );
    }

    #[test]
    fn validate_rejects_duplicate_object_property_mappings() {
        use crate::mapping::{ColumnRef, ObjectMappingDef, PropertyLocation, PropertyMappingDef};

        let mut ontology = base_ontology();
        let mut mapping = ObjectMappingDef::new("om-user", "node-user", "pg-main", "users");
        mapping.property_mappings.push(PropertyMappingDef {
            property_id: "prop-email".into(),
            property_key: PropertyKey::new("email").expect("valid property key"),
            location: PropertyLocation::Column(ColumnRef::new("users", "email")),
            transform: Default::default(),
            concept_map_id: None,
        });
        mapping.property_mappings.push(PropertyMappingDef {
            property_id: "prop-email".into(),
            property_key: PropertyKey::new("email").expect("valid property key"),
            location: PropertyLocation::Column(ColumnRef::new("users", "email_address")),
            transform: Default::default(),
            concept_map_id: None,
        });
        ontology.object_mappings.push(mapping);
        ontology.rebuild_indices().expect("rebuild");

        let errors = ontology.validate();

        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.object_mapping.duplicate_property_mapping"),
            "expected object mapping duplicate property diagnostic: {errors:?}"
        );
    }

    #[test]
    fn validate_rejects_object_property_mapping_with_unknown_concept_map() {
        use crate::mapping::{ColumnRef, ObjectMappingDef, PropertyLocation, PropertyMappingDef};

        let mut ontology = base_ontology();
        let mut mapping = ObjectMappingDef::new("om-user", "node-user", "pg-main", "users");
        mapping.property_mappings.push(PropertyMappingDef {
            property_id: "prop-email".into(),
            property_key: PropertyKey::new("email").expect("valid property key"),
            location: PropertyLocation::Column(ColumnRef::new("users", "email")),
            transform: Default::default(),
            concept_map_id: Some(ConceptMapId::new("cm-missing")),
        });
        ontology.object_mappings.push(mapping);
        ontology.rebuild_indices().expect("rebuild");

        let errors = ontology.validate();

        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.object_mapping.unknown_concept_map_id"),
            "expected object mapping unknown concept map diagnostic: {errors:?}"
        );
    }

    #[test]
    fn validate_rejects_link_mapping_with_invalid_endpoint_shape() {
        use crate::mapping::{
            EndpointRef, JoinCostHint, LinkCardinality, LinkMappingDef, LinkMappingId,
            LinkMappingKind, SourceId,
        };

        let mut ontology = base_ontology();
        ontology.link_mappings.push(LinkMappingDef {
            id: LinkMappingId::new("lm-bad"),
            edge_type_id: EdgeTypeId::new("edge-owns"),
            kind: LinkMappingKind::Computed {
                predicate: "users.id = users.owner_id".into(),
            },
            source_endpoint: EndpointRef {
                source_id: SourceId::new(""),
                relation: "".into(),
                key_columns: Vec::new(),
            },
            target_endpoint: EndpointRef {
                source_id: SourceId::new("pg-main"),
                relation: "users".into(),
                key_columns: vec!["id".into(), "id".into()],
            },
            join_cost_hint: JoinCostHint::Unknown,
            precedence: 0,
            cardinality: LinkCardinality::ManyToMany,
        });
        ontology.rebuild_indices().expect("rebuild");

        let errors = ontology.validate();

        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.link_mapping.empty_endpoint_source_id"),
            "expected empty endpoint source diagnostic: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.link_mapping.empty_endpoint_relation"),
            "expected empty endpoint relation diagnostic: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.link_mapping.empty_endpoint_key"),
            "expected empty endpoint key diagnostic: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.link_mapping.duplicate_endpoint_key_column"),
            "expected duplicate endpoint key diagnostic: {errors:?}"
        );
    }

    #[test]
    fn validate_rejects_link_mapping_with_invalid_kind_shape() {
        use crate::mapping::{
            ColumnRef, EndpointRef, JoinCostHint, LinkCardinality, LinkMappingDef, LinkMappingId,
            LinkMappingKind, SourceId, SourceRelationRef,
        };

        let mut ontology = base_ontology();
        let endpoint = EndpointRef {
            source_id: SourceId::new("pg-main"),
            relation: "users".into(),
            key_columns: vec!["id".into(), "tenant_id".into()],
        };
        ontology.link_mappings.push(LinkMappingDef {
            id: LinkMappingId::new("lm-bad-bridge"),
            edge_type_id: EdgeTypeId::new("edge-owns"),
            kind: LinkMappingKind::Bridge {
                bridge_relation: SourceRelationRef {
                    source_id: SourceId::new("pg-main"),
                    relation: "".into(),
                    kind: Default::default(),
                },
                source_join: vec![ColumnRef::new("user_edges", "user_id")],
                target_join: vec![
                    ColumnRef::new("user_edges", "owner_id"),
                    ColumnRef::new("", "tenant_id"),
                ],
                bridge_workspace_scope: Some(ColumnRef::new("user_edges", "")),
            },
            source_endpoint: endpoint.clone(),
            target_endpoint: endpoint,
            join_cost_hint: JoinCostHint::Unknown,
            precedence: 0,
            cardinality: LinkCardinality::ManyToMany,
        });
        ontology.link_mappings.push(LinkMappingDef {
            id: LinkMappingId::new("lm-empty-computed"),
            edge_type_id: EdgeTypeId::new("edge-owns"),
            kind: LinkMappingKind::Computed {
                predicate: " ".into(),
            },
            source_endpoint: EndpointRef {
                source_id: SourceId::new("pg-main"),
                relation: "users".into(),
                key_columns: vec!["id".into()],
            },
            target_endpoint: EndpointRef {
                source_id: SourceId::new("pg-main"),
                relation: "users".into(),
                key_columns: vec!["id".into()],
            },
            join_cost_hint: JoinCostHint::Unknown,
            precedence: 0,
            cardinality: LinkCardinality::ManyToMany,
        });
        ontology.rebuild_indices().expect("rebuild");

        let errors = ontology.validate();

        for code in [
            "ontology.validate.link_mapping.invalid_bridge_relation",
            "ontology.validate.link_mapping.source_bridge_join_arity_mismatch",
            "ontology.validate.link_mapping.invalid_bridge_join_column",
            "ontology.validate.link_mapping.invalid_bridge_workspace_scope",
            "ontology.validate.link_mapping.empty_computed_predicate",
        ] {
            assert!(
                errors.iter().any(|e| e.code == code),
                "expected {code} diagnostic: {errors:?}"
            );
        }
    }

    #[test]
    fn validate_accepts_measure_aggregation_role_on_numeric_property() {
        let mut ontology = base_ontology();
        // Swap first property's type to Int so Measure is valid.
        ontology.node_types[0].properties[0].property_type = ox_core::types::PropertyType::Int;
        ontology.node_types[0].properties[0].aggregation_role = Some(AggregationRole::Measure);

        let errors = ontology.validate();

        assert!(
            !errors
                .iter()
                .any(|e| e.code == "ontology.validate.property.measure_non_numeric"),
            "Int+Measure must validate cleanly: {errors:?}"
        );
    }
}
