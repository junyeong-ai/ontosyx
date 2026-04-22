use ox_core::types::PropertyType;

use super::{
    AggregationRole, IndexDef, NodeConstraint, NodeTypeDef, NodeTypeId, OntologyIR, PropertyDef,
    PropertyId,
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
    errors: &mut Vec<String>,
) {
    let mut seen_ids = std::collections::HashSet::<&PropertyId>::new();
    let mut seen_names = std::collections::HashSet::new();

    for property in properties {
        if property.id.trim().is_empty() {
            errors.push(format!(
                "{owner_kind} '{owner_label}' has a property with an empty id"
            ));
        } else if !seen_ids.insert(&property.id) {
            errors.push(format!(
                "{owner_kind} '{owner_label}' has duplicate property id '{}'",
                property.id
            ));
        }

        // `PropertyKey` enforces the non-empty / Cypher-safe invariants
        // at construction; only the placeholder sentinel and duplicate
        // detection need checking here.
        let name = property.name.as_str();
        if name == LABEL_PLACEHOLDER {
            errors.push(format!(
                "{owner_kind} '{owner_label}' has a property with the \
                 `Default::default()` placeholder name — struct-update \
                 callers must override `name` explicitly"
            ));
            continue;
        }

        if !seen_names.insert(name.to_string()) {
            errors.push(format!(
                "{owner_kind} '{owner_label}' has duplicate property '{name}'"
            ));
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
            errors.push(format!(
                "{owner_kind} '{owner_label}' property '{name}' has \
                 aggregation_role=Measure but a non-numeric property_type; \
                 Measure implies SUM/AVG/MAX semantics"
            ));
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
    errors: &mut Vec<String>,
) {
    if property_ids.is_empty() {
        errors.push(format!(
            "Node '{}' has an empty {constraint_name} constraint",
            node.label
        ));
        return;
    }

    let mut seen = std::collections::HashSet::<&str>::new();
    for prop_id in property_ids {
        let id = prop_id.trim();
        if id.is_empty() {
            errors.push(format!(
                "Node '{}' has a {constraint_name} constraint with an empty property id",
                node.label
            ));
            continue;
        }

        if !seen.insert(id) {
            errors.push(format!(
                "Node '{}' has duplicate property id '{}' in a {constraint_name} constraint",
                node.label, id
            ));
        }

        match property_def_by_id(&node.properties, id) {
            Some(def) => {
                if require_non_nullable && def.nullable {
                    errors.push(format!(
                        "Node '{}' constraint '{}' requires non-nullable property '{}'",
                        node.label, constraint_name, def.name
                    ));
                }
            }
            None => errors.push(format!(
                "Node '{}' constraint references unknown property id '{}'",
                node.label, id
            )),
        }
    }
}

fn validate_index_target(
    node_types: &[NodeTypeDef],
    node_id: &NodeTypeId,
    property_ids: &[PropertyId],
    index_name: &str,
    errors: &mut Vec<String>,
) {
    let Some(node) = node_types.iter().find(|node| node.id == *node_id) else {
        errors.push(format!(
            "Index '{}' references unknown node id '{}'",
            index_name, node_id
        ));
        return;
    };

    if property_ids.is_empty() {
        errors.push(format!(
            "Index '{}' on node '{}' must reference at least one property",
            index_name, node.label
        ));
        return;
    }

    let mut seen = std::collections::HashSet::<&str>::new();
    for prop_id in property_ids {
        let id = prop_id.trim();
        if id.is_empty() {
            errors.push(format!(
                "Index '{}' on node '{}' contains an empty property id",
                index_name, node.label
            ));
            continue;
        }

        if !seen.insert(id) {
            errors.push(format!(
                "Index '{}' on node '{}' contains duplicate property id '{}'",
                index_name, node.label, id
            ));
        }

        if property_def_by_id(&node.properties, id).is_none() {
            errors.push(format!(
                "Index '{}' references unknown property id '{}' on node '{}'",
                index_name, id, node.label
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

impl OntologyIR {
    /// Validate internal consistency of the ontology.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if self.id.trim().is_empty() {
            errors.push("Ontology id must not be empty".to_string());
        }
        if self.name.trim().is_empty() {
            errors.push("Ontology name must not be empty".to_string());
        }
        if self.node_types.is_empty() {
            errors.push("Ontology must define at least one node type".to_string());
        }

        let mut seen_node_ids = std::collections::HashSet::<NodeTypeId>::new();
        let mut seen_node_labels = std::collections::HashSet::new();

        for node in &self.node_types {
            // Validate node id
            if node.id.trim().is_empty() {
                errors.push("Node type id must not be empty".to_string());
            } else if !seen_node_ids.insert(node.id.clone()) {
                errors.push(format!("Duplicate node type id: '{}'", node.id));
            }

            // `GraphLabel` already rejects empty / invalid identifiers
            // at construction time, so validation here reduces to: the
            // placeholder sentinel (caller forgot to override the
            // Default::default() label), and duplicate-label detection.
            let label = node.label.as_str();
            if label == LABEL_PLACEHOLDER {
                errors.push(format!(
                    "Node type '{}' has the `Default::default()` placeholder label — \
                     struct-update callers must override `label` explicitly",
                    node.id
                ));
                continue;
            }

            if !seen_node_labels.insert(label.to_string()) {
                errors.push(format!("Duplicate node type label: '{label}'"));
            }

            validate_property_defs("Node", label, &node.properties, &mut errors);

            for constraint_def in &node.constraints {
                if constraint_def.id.trim().is_empty() {
                    errors.push(format!(
                        "Node '{}' has a constraint with an empty id",
                        node.label
                    ));
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
                errors.push("Edge type id must not be empty".to_string());
            }

            // Parallel to the node case above: `GraphLabel` enforces
            // the identifier invariants at construction, so validation
            // here only has to catch the sentinel and duplicates.
            let label = edge.label.as_str();
            if label == LABEL_PLACEHOLDER {
                errors.push(format!(
                    "Edge type '{}' has the `Default::default()` placeholder label — \
                     struct-update callers must override `label` explicitly",
                    edge.id
                ));
                continue;
            }
            if edge.source_node_id.trim().is_empty() || edge.target_node_id.trim().is_empty() {
                errors.push(format!(
                    "Edge '{}' must define both source_node_id and target_node_id",
                    edge.label
                ));
            }
            if !seen_edge_signatures.insert((
                edge.label.clone(),
                edge.source_node_id.clone(),
                edge.target_node_id.clone(),
            )) {
                errors.push(format!(
                    "Duplicate edge type definition: '{}({}->{})'",
                    edge.label, edge.source_node_id, edge.target_node_id
                ));
            }

            validate_property_defs("Edge", &edge.label, &edge.properties, &mut errors);

            if !seen_node_ids.contains::<str>(&edge.source_node_id) {
                errors.push(format!(
                    "Edge '{}' references unknown source node id '{}'",
                    edge.label, edge.source_node_id
                ));
            }
            if !seen_node_ids.contains::<str>(&edge.target_node_id) {
                errors.push(format!(
                    "Edge '{}' references unknown target node id '{}'",
                    edge.label, edge.target_node_id
                ));
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
                    errors.push(format!(
                        "Node '{}' implements unknown interface id '{}'",
                        node.label, if_id
                    ));
                }
            }
            for act_id in &node.actions {
                if !self.lookup.action_id_idx.contains_key(act_id) {
                    errors.push(format!(
                        "Node '{}' references unknown action id '{}'",
                        node.label, act_id
                    ));
                }
            }
            for met_id in &node.metrics {
                if !self.lookup.metric_id_idx.contains_key(met_id) {
                    errors.push(format!(
                        "Node '{}' references unknown metric id '{}'",
                        node.label, met_id
                    ));
                }
            }
            for rule_id in &node.rules {
                if !self.lookup.rule_id_idx.contains_key(rule_id) {
                    errors.push(format!(
                        "Node '{}' references unknown rule id '{}'",
                        node.label, rule_id
                    ));
                }
            }

            // Property-level Phase 5-B links.
            for prop in &node.properties {
                if let Some(term_id) = &prop.glossary_term_id
                    && !self.lookup.glossary_term_id_idx.contains_key(term_id)
                {
                    errors.push(format!(
                        "Property '{}.{}' references unknown glossary term id '{}'",
                        node.label, prop.name, term_id
                    ));
                }
                if let Some(fn_id) = &prop.derived_from {
                    if !self.lookup.function_id_idx.contains_key(fn_id) {
                        errors.push(format!(
                            "Property '{}.{}' is derived_from unknown function id '{}'",
                            node.label, prop.name, fn_id
                        ));
                    }
                    // derived_from and source_column are mutually
                    // exclusive: a value cannot both be computed and
                    // read from a physical column. The planner refuses
                    // such a PropertyDef at compile time; catching it
                    // here gives authors a clearer error.
                    if prop.source_column.is_some() {
                        errors.push(format!(
                            "Property '{}.{}' declares both `derived_from` and \
                             `source_column` — pick one",
                            node.label, prop.name
                        ));
                    }
                }
                // Ω-3: value_set_id referential integrity.
                if let Some(vs_id) = &prop.value_set_id
                    && !self.lookup.value_set_id_idx.contains_key(vs_id)
                {
                    errors.push(format!(
                        "Property '{}.{}' references unknown value set id '{}'",
                        node.label, prop.name, vs_id
                    ));
                }
                // Ω-4: notation_pattern_id referential integrity.
                if let Some(np_id) = &prop.notation_pattern_id
                    && !self.lookup.notation_pattern_id_idx.contains_key(np_id)
                {
                    errors.push(format!(
                        "Property '{}.{}' references unknown notation pattern id '{}'",
                        node.label, prop.name, np_id
                    ));
                }
                // Ω-6: unit_id must point at a CodedValue registered
                // in the global coded_value index. Any CodeSystem
                // can supply units; UCUM is the canonical external
                // system but domain-specific systems are valid too.
                if let Some(unit_id) = &prop.unit_id
                    && !self.lookup.coded_value_loc.contains_key(unit_id)
                {
                    errors.push(format!(
                        "Property '{}.{}' references unknown unit (CodedValueId '{}')",
                        node.label, prop.name, unit_id
                    ));
                }
                // Ω-7: value_range_set_id referential integrity.
                if let Some(rs_id) = &prop.value_range_set_id
                    && !self.lookup.value_range_set_id_idx.contains_key(rs_id)
                {
                    errors.push(format!(
                        "Property '{}.{}' references unknown value range set id '{}'",
                        node.label, prop.name, rs_id
                    ));
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
                errors.push(format!(
                    "Rule '{}' activates on unknown action id '{}'",
                    rule.name, action_id
                ));
            }
        }

        // Action precondition / postcondition ids must reference real
        // rules. Same rationale as rule activations.
        for action in &self.actions {
            for rule_id in action.preconditions.iter().chain(action.postconditions.iter()) {
                if !self.lookup.rule_id_idx.contains_key(rule_id) {
                    errors.push(format!(
                        "Action '{}' references unknown rule id '{}' in its pre/post conditions",
                        action.name, rule_id
                    ));
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
                errors.push(format!(
                    "Object mapping '{}' targets unknown node type id '{}'",
                    om.id, om.node_type_id
                ));
            }
        }
        for lm in &self.link_mappings {
            if !self.lookup.edge_id_idx.contains_key(&lm.edge_type_id) {
                errors.push(format!(
                    "Link mapping '{}' targets unknown edge type id '{}'",
                    lm.id, lm.edge_type_id
                ));
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
                errors.push(format!(
                    "Link mapping '{}' uses Bridge kind but declares \
                     cardinality={:?}; Bridge typically implies \
                     ManyToMany — verify the override is intentional",
                    lm.id, lm.cardinality
                ));
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
                        errors.push(format!(
                            "Vector index on node '{}' property '{}' must have dimensions > 0",
                            node_id, property_id
                        ));
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
        use crate::integrity::{RegistryReferenceCheck, render_dangling_references};
        let dangling = self.dangling_references();
        if !dangling.is_empty() {
            errors.extend(render_dangling_references(&dangling));
        }

        errors
    }
}

#[cfg(test)]
mod tests {
    use ox_core::graph_label::GraphLabel;
    use ox_core::i18n::LocalizedText;
    use crate::ir::*;
    use ox_core::property_key::PropertyKey;
    use ox_core::types::PropertyType;

    fn property(id: &str, name: &str, nullable: bool) -> PropertyDef {
        PropertyDef {
            id: id.into(),
            name: PropertyKey::new(name).expect("test property name must be valid"),
            property_type: PropertyType::String,
            nullable,
            default_value: None,
            description: LocalizedText::default(),
            classification: None,
            ..Default::default()
        }
    }

    fn base_ontology() -> OntologyIR {
        OntologyIR::new(
            "test".to_string(),
            "Test".to_string(),
            LocalizedText::default(),
            1,
            vec![NodeTypeDef {
                id: "node-user".into(),
                label: GraphLabel::new("User").expect("User is a valid label"),
                description: LocalizedText::default(),
                properties: vec![
                    property("prop-id", "id", false),
                    property("prop-email", "email", false),
                ],
                constraints: vec![
                    ConstraintDef {
                        id: "cst-unique-email".into(),
                        constraint: NodeConstraint::Unique {
                            property_ids: vec!["prop-email".into()],
                        },
                    },
                    ConstraintDef {
                        id: "cst-exists-id".into(),
                        constraint: NodeConstraint::Exists {
                            property_id: "prop-id".into(),
                        },
                    },
                ],
                ..Default::default()
            }],
            vec![EdgeTypeDef {
                id: "edge-owns".into(),
                label: GraphLabel::new("OWNS").expect("OWNS is valid"),
                description: LocalizedText::default(),
                source_node_id: "node-user".into(),
                target_node_id: "node-user".into(),
                properties: vec![],
                cardinality: Cardinality::OneToMany,
                ..Default::default()
            }],
            vec![IndexDef::Single {
                id: "idx-user-email".to_string(),
                node_id: "node-user".into(),
                property_id: "prop-email".into(),
            }],
        )
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
                .any(|error| error.contains("duplicate property 'email'"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("unknown property id 'prop-missing'"))
        );
    }

    #[test]
    fn validate_rejects_nullable_required_constraints() {
        let mut ontology = base_ontology();
        ontology.node_types[0].properties[0].nullable = true;

        let errors = ontology.validate();

        assert!(errors.iter().any(|error| {
            error.contains("constraint 'exists' requires non-nullable property 'id'")
        }));
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

        assert!(errors.iter().any(|e| e.contains("id must not be empty")));
        assert!(errors.iter().any(|e| e.contains("name must not be empty")));
        assert!(errors.iter().any(|e| e.contains("at least one node type")));
    }

    #[test]
    fn validate_rejects_edge_referencing_unknown_node_id() {
        let mut ontology = base_ontology();
        ontology.edge_types[0].source_node_id = "node-nonexistent".into();

        let errors = ontology.validate();

        assert!(
            errors
                .iter()
                .any(|e| e.contains("unknown source node id 'node-nonexistent'"))
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
                .any(|e| e.contains("unknown property id 'prop-nonexistent'"))
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
                .any(|e| e.contains("aggregation_role=Measure")
                    && e.contains("non-numeric")),
            "expected Measure/non-numeric warning, got: {errors:?}"
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
            !errors.iter().any(|e| e.contains("aggregation_role=Measure")),
            "Int+Measure must validate cleanly: {errors:?}"
        );
    }
}
