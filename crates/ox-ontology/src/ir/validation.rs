use ox_core::diagnostic::{diag, DiagnosticMessage};
use ox_core::types::PropertyType;

use super::{
    AggregationRole, EdgeKind, IndexDef, NodeConstraint, NodeTypeDef, NodeTypeId, OntologyIR,
    PropertyDef, PropertyId,
};

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
        // glossary anchor — the kind of silent semantic drop the IR
        // is meant to prevent. `binding_exempt` is the explicit
        // opt-out for legitimate cases (PK / audit timestamp /
        // opaque id) so an audit can find every exemption by name
        // instead of reading commit history. `Identifier`-role
        // properties get an implicit exemption: they're identity by
        // design and the IR already names that role separately.
        let is_identifier =
            matches!(property.aggregation_role, Some(AggregationRole::Identifier));
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
        let mut errors: Vec<DiagnosticMessage> = Vec::new();

        if self.id.trim().is_empty() {
            errors.push(
                diag("ontology.validate.id.empty").message("Ontology id must not be empty"),
            );
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
            errors.push(
                diag("ontology.validate.no_content").message(
                    "Ontology must populate at least one collection \
                     (node_types / edge_types / glossary / rules / code_systems / mappings)",
                ),
            );
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

            for term_id in &edge.glossary_anchors {
                if !self.lookup.glossary_term_id_idx.contains_key(term_id) {
                    errors.push(
                        diag("ontology.validate.edge.unknown_glossary_anchor")
                            .with("label", label)
                            .with("glossary_term_id", term_id.as_str())
                            .message(format!(
                                "Edge '{}' anchors unknown glossary term id '{}'",
                                edge.label, term_id
                            )),
                    );
                }
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
        // Phase 5-B + 4-A referential integrity.
        //
        // Every id link that NodeTypeDef / PropertyDef acquired in
        // Phase 5-B, plus every object / link mapping id, must
        // resolve against a matching *Def. Phase 5-D replaced the
        // `HashSet`-per-validate pattern with the precomputed
        // lookup indices on `self.lookup` — the check now runs in
        // O(1) per reference instead of O(N) alloc + O(1) lookup.
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
            for term_id in &node.glossary_anchors {
                if !self.lookup.glossary_term_id_idx.contains_key(term_id) {
                    errors.push(
                        diag("ontology.validate.node.unknown_glossary_anchor")
                            .with("node_label", node.label.as_str())
                            .with("glossary_term_id", term_id.as_str())
                            .message(format!(
                                "Node '{}' anchors unknown glossary term id '{}'",
                                node.label, term_id
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
                    if let Some(msg) = self.check_binding_target_exists(
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
            for rule_id in action.preconditions.iter().chain(action.postconditions.iter()) {
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
        // Mapping (ADR 0003) referential integrity. O(1) per mapping
        // via `node_id_idx` / `edge_id_idx` instead of a per-validate
        // HashSet alloc.
        // -------------------------------------------------------------
        for om in &self.object_mappings {
            if !self.lookup.node_id_idx.contains_key(&om.node_type_id) {
                errors.push(
                    diag("ontology.validate.object_mapping.unknown_node_type_id")
                        .with("object_mapping_id", om.id.as_str())
                        .with("node_type_id", om.node_type_id.as_str())
                        .message(format!(
                            "Object mapping '{}' targets unknown node type id '{}'",
                            om.id, om.node_type_id
                        )),
                );
            }
        }
        for lm in &self.link_mappings {
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
        // Phase 1.7 — registry cross-reference integrity.
        //
        // The dedicated module walks every `Option<XxxId>` pointer on
        // PropertyDef / RuleDef / ValueSetDef / ConceptMapDef and
        // flags any id that does not resolve. Using the trait here
        // keeps the walker reusable from ad-hoc edit helpers without
        // round-tripping through the full `validate()` surface.
        use crate::integrity::{render_dangling_references, RegistryReferenceCheck};
        let dangling = self.dangling_references();
        if !dangling.is_empty() {
            errors.extend(render_dangling_references(&dangling));
        }

        // -------------------------------------------------------------
        // ADR-0015 — segment referential integrity.
        //
        // A `SegmentDef` declares a named membership predicate over
        // a single NodeType. Two invariants the rest of the system
        // depends on: the target NodeType resolves, and every
        // property the filter mentions exists on that target.
        // Drift here would break the glossary realisation chain
        // (ADR-0014) and the runtime's segment-aware compile pass.
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
            let known_properties: std::collections::HashSet<&str> = target
                .properties
                .iter()
                .map(|p| p.name.as_str())
                .collect();
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
            PropertyBinding::Glossary { id, .. } => {
                if self.lookup.glossary_term_id_idx.contains_key(id) {
                    return None;
                }
                ("glossary term", id.as_str().to_string())
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
            // ValueRange and Glossary variants don't carry a strength
            // field — they cannot be `Required` and are unreachable
            // from the strength()==Required gate above.
            PropertyBinding::ValueRange { .. } | PropertyBinding::Glossary { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ir::*;
    use ox_core::i18n::LocalizedText;

    // Shared fixture helpers — validation tests and the sibling
    // `ir::tests` module both draw from `test_fixtures` so the
    // starting ontology stays byte-identical across modules.
    // Diverging the two fixtures silently in the past has caused
    // tests to pass on one shape while failing on the other.
    use crate::test_fixtures::{property_nullable as property, sample_user_ontology};

    fn base_ontology() -> OntologyIR {
        sample_user_ontology()
    }

    #[test]
    fn validate_accepts_well_formed_ontology() {
        let ontology = base_ontology();
        assert!(ontology.validate().is_empty());
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

        assert!(errors.iter().any(|e| e.code == "ontology.validate.id.empty"));
        assert!(errors.iter().any(|e| e.code == "ontology.validate.name.empty"));
        // Empty everything also fails the "populate at least one
        // collection" invariant.
        assert!(errors.iter().any(|e| e.code == "ontology.validate.no_content"));
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
        ontology.node_types[0].properties[0].aggregation_role =
            Some(AggregationRole::Measure);
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
    fn validate_rejects_unknown_glossary_anchor_on_node() {
        let mut ontology = base_ontology();
        ontology.node_types[0]
            .glossary_anchors
            .push(crate::glossary::GlossaryTermId::new("gt-nonexistent"));

        let errors = ontology.validate();

        assert!(
            errors
                .iter()
                .any(|e| e.code == "ontology.validate.node.unknown_glossary_anchor"),
            "expected unknown_glossary_anchor diagnostic: {errors:?}"
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
            errors.iter().any(|e| e.code
                == "ontology.validate.edge.composition_requires_singular_source"),
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
            !errors.iter().any(|e| e.code
                == "ontology.validate.edge.composition_requires_singular_source"),
            "OneToMany composition is the canonical case: {errors:?}"
        );
    }

    // ADR-0015 — segment referential integrity.

    #[test]
    fn validate_accepts_segment_targeting_real_node_with_real_properties() {
        use crate::segment::{SegmentDef, SegmentFilter, SegmentLiteral};
        use ox_core::PropertyKey;

        let mut ontology = base_ontology();
        let target_node_id = ontology.node_types[0].id.clone();
        let target_property =
            ontology.node_types[0].properties[0].name.as_str().to_string();

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
            errors.iter().any(|e| e.code
                == "ontology.validate.rule.derived_origin_missing_binding"),
            "expected derived-rule orphan diagnostic: {errors:?}"
        );
    }

    #[test]
    fn validate_accepts_measure_aggregation_role_on_numeric_property() {
        let mut ontology = base_ontology();
        // Swap first property's type to Int so Measure is valid.
        ontology.node_types[0].properties[0].property_type =
            ox_core::types::PropertyType::Int;
        ontology.node_types[0].properties[0].aggregation_role =
            Some(AggregationRole::Measure);

        let errors = ontology.validate();

        assert!(
            !errors
                .iter()
                .any(|e| e.code == "ontology.validate.property.measure_non_numeric"),
            "Int+Measure must validate cleanly: {errors:?}"
        );
    }
}
