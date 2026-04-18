use ox_core::ontology_ir::{Cardinality, NodeConstraint, NodeTypeDef, OntologyIR};
use ox_core::types::PropertyType;

/// Generate a GraphQL schema from an OntologyIR.
///
/// Honours Phase A Palantir fields:
/// - `deprecated_at` on properties / edges → `@deprecated(reason: ...)` field directive.
/// - `replaced_by_id` → fed into the deprecation reason so consumers see the successor inline.
/// - `deprecated_at` on a node type → `[DEPRECATED]` marker in the type's docstring,
///   because the GraphQL spec restricts `@deprecated` to fields and enum values.
pub fn generate_graphql(ontology: &OntologyIR) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push(format!("# GraphQL Schema for: {}", ontology.name));
    if let Some(desc) = ontology.description.present() {
        lines.push(format!("# {desc}"));
    }
    lines.push(String::new());

    for node in ontology.node_types() {
        // Determine if any property is a PK (unique constraint -> ID type)
        let pk_name = find_pk_property(node);

        // Type-level docstring carries description + a [DEPRECATED] marker
        // when the node has `deprecated_at` set, since GraphQL @deprecated
        // is not legal on object type definitions.
        let deprecation_marker = if node.deprecated_at.is_some() {
            "[DEPRECATED] "
        } else {
            ""
        };
        match (deprecation_marker.is_empty(), node.description.present()) {
            (true, Some(desc)) => lines.push(format!("\"\"\"{desc}\"\"\"")),
            (false, Some(desc)) => lines.push(format!("\"\"\"{deprecation_marker}{desc}\"\"\"")),
            (false, None) => lines.push("\"\"\"[DEPRECATED]\"\"\"".into()),
            (true, None) => {}
        }
        lines.push(format!("type {} {{", graphql_safe_name(&node.label)));

        // Properties
        for prop in &node.properties {
            let is_pk = Some(prop.name.as_str()) == pk_name;
            let gql_type = if is_pk {
                "ID!".to_string()
            } else {
                graphql_type(&prop.property_type, prop.nullable)
            };
            let deprecated_directive = property_deprecation_directive(prop, ontology);
            let desc_comment = prop
                .description
                .present()
                .map(|d| format!("  # {d}"))
                .unwrap_or_default();
            lines.push(format!(
                "  {}: {}{}{}",
                graphql_safe_name(&prop.name),
                gql_type,
                deprecated_directive,
                desc_comment,
            ));
        }

        // Relationship fields (edges where this node is source or target)
        emit_graphql_relationships(&mut lines, ontology, node);

        lines.push("}".to_string());
        lines.push(String::new());
    }

    lines.join("\n")
}

/// Build the `@deprecated(reason: "...")` directive for a property field, or
/// an empty string when the property is not deprecated. Resolves
/// `replaced_by_id` against the ontology so the reason names the successor.
fn property_deprecation_directive(
    prop: &ox_core::ontology_ir::PropertyDef,
    ontology: &OntologyIR,
) -> String {
    if prop.deprecated_at.is_none() {
        return String::new();
    }
    let reason = match &prop.replaced_by_id {
        Some(id) => match ontology.property_by_id(id.as_ref()) {
            Some((_, replacement)) => {
                format!("Replaced by `{}`", replacement.name)
            }
            None => "Deprecated".into(),
        },
        None => "Deprecated".into(),
    };
    format!(
        " @deprecated(reason: \"{}\")",
        escape_directive_string(&reason)
    )
}

/// Build the `@deprecated(reason: "...")` directive for a relationship field
/// derived from an edge type, or an empty string when the edge is not
/// deprecated. Resolves `replaced_by_id` against the edge index.
fn edge_deprecation_directive(
    edge: &ox_core::ontology_ir::EdgeTypeDef,
    ontology: &OntologyIR,
) -> String {
    if edge.deprecated_at.is_none() {
        return String::new();
    }
    let reason = match &edge.replaced_by_id {
        Some(id) => match ontology.edge_by_id(id.as_ref()) {
            Some(replacement) => format!("Replaced by `{}`", replacement.label),
            None => "Deprecated".into(),
        },
        None => "Deprecated".into(),
    };
    format!(
        " @deprecated(reason: \"{}\")",
        escape_directive_string(&reason)
    )
}

/// Escape characters that would break a GraphQL string literal inside a
/// directive argument. Only `"` and `\` need escaping for one-line reasons.
fn escape_directive_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn graphql_type(pt: &PropertyType, nullable: bool) -> String {
    let base = match pt {
        PropertyType::Bool => "Boolean".to_string(),
        PropertyType::Int => "Int".to_string(),
        PropertyType::Float => "Float".to_string(),
        PropertyType::String => "String".to_string(),
        PropertyType::Date => "Date".to_string(),
        PropertyType::DateTime => "DateTime".to_string(),
        PropertyType::Duration => "String".to_string(),
        PropertyType::Bytes => "String".to_string(),
        PropertyType::List { element } => {
            format!("[{}!]", graphql_type(element, false))
        }
        PropertyType::Map => "JSON".to_string(),
    };
    if nullable { base } else { format!("{base}!") }
}

fn graphql_safe_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    // GraphQL names must start with a letter or underscore
    if sanitized.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("_{sanitized}")
    } else {
        sanitized
    }
}

/// Determine the PK property name for a node (first property in a Unique or NodeKey constraint).
fn find_pk_property(node: &NodeTypeDef) -> Option<&str> {
    for cdef in &node.constraints {
        match &cdef.constraint {
            NodeConstraint::Unique { property_ids } | NodeConstraint::NodeKey { property_ids } => {
                if let Some(pid) = property_ids.first()
                    && let Some(prop) = node.properties.iter().find(|p| p.id == *pid)
                {
                    return Some(&prop.name);
                }
            }
            NodeConstraint::Exists { .. } => {}
        }
    }
    None
}

fn emit_graphql_relationships(lines: &mut Vec<String>, ontology: &OntologyIR, node: &NodeTypeDef) {
    for edge in ontology.edge_types() {
        let deprecation = edge_deprecation_directive(edge, ontology);
        // Outgoing edges (source = this node)
        if edge.source_node_id == node.id
            && let Some(target) = ontology.node_by_id(&edge.target_node_id)
        {
            // Prefer `target_role` for the outgoing field name when the
            // ontology designer pinned one — `customer.subscriptions` reads
            // better than `customer.has_subscription`. Falls back to the
            // existing `<edge>_<target>` scheme otherwise.
            let field_name = edge
                .target_role
                .as_deref()
                .map(graphql_safe_name)
                .unwrap_or_else(|| relationship_field_name(&edge.label, &target.label, true));
            let is_many = matches!(
                edge.cardinality,
                Cardinality::OneToMany | Cardinality::ManyToMany
            );
            let target_type = graphql_safe_name(&target.label);
            let gql_type = if is_many {
                format!("[{target_type}!]!")
            } else {
                format!("{target_type}!")
            };
            lines.push(format!(
                "  {field_name}: {gql_type}{deprecation}  @relationship(type: \"{}\", direction: OUT)",
                edge.label
            ));
        }
        // Incoming edges (target = this node)
        if edge.target_node_id == node.id
            && let Some(source) = ontology.node_by_id(&edge.source_node_id)
        {
            // Incoming field: the source role describes who's holding us.
            let field_name = edge
                .source_role
                .as_deref()
                .map(graphql_safe_name)
                .unwrap_or_else(|| relationship_field_name(&edge.label, &source.label, false));
            let is_many = matches!(
                edge.cardinality,
                Cardinality::ManyToOne | Cardinality::ManyToMany
            );
            let source_type = graphql_safe_name(&source.label);
            let gql_type = if is_many {
                format!("[{source_type}!]!")
            } else {
                format!("{source_type}!")
            };
            lines.push(format!(
                "  {field_name}: {gql_type}{deprecation}  @relationship(type: \"{}\", direction: IN)",
                edge.label
            ));
        }
    }
}

/// Generate a field name from a relationship label.
fn relationship_field_name(edge_label: &str, related_label: &str, outgoing: bool) -> String {
    let base = edge_label.to_lowercase();
    let related = related_label.to_lowercase().replace(' ', "_");
    if outgoing {
        graphql_safe_name(&format!("{base}_{related}"))
    } else {
        graphql_safe_name(&format!("{related}_{base}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::GraphLabel;
    use ox_core::LocalizedText;
    use ox_core::PropertyKey;
    use ox_core::ontology_ir::{
        Cardinality, ConstraintDef, EdgeTypeDef, NodeConstraint, NodeTypeDef, OntologyIR,
        PropertyDef,
    };

    fn gl(s: &'static str) -> GraphLabel {
        GraphLabel::new(s).expect("test label literal must be valid")
    }

    fn pk(s: &'static str) -> PropertyKey {
        PropertyKey::new(s).expect("test property name literal must be valid")
    }
    fn sample_ontology() -> OntologyIR {
        OntologyIR::new(
            "g-test".into(),
            "Sales".into(),
            LocalizedText::new("Sales domain"),
            1,
            vec![
                NodeTypeDef {
                    id: "n1".into(),
                    label: gl("Customer"),
                    description: LocalizedText::new("A buyer"),
                    properties: vec![
                        PropertyDef {
                            id: "p1".into(),
                            name: pk("id"),
                            property_type: PropertyType::String,
                            nullable: false,
                            default_value: None,
                            description: LocalizedText::default(),
                            classification: None,
                            ..Default::default()
                        },
                        PropertyDef {
                            id: "p2".into(),
                            name: pk("email"),
                            property_type: PropertyType::String,
                            nullable: true,
                            default_value: None,
                            description: LocalizedText::new("Contact email"),
                            classification: None,
                            ..Default::default()
                        },
                    ],
                    constraints: vec![ConstraintDef {
                        id: "c1".into(),
                        constraint: NodeConstraint::Unique {
                            property_ids: vec!["p1".into()],
                        },
                    }],
                    ..Default::default()
                },
                NodeTypeDef {
                    id: "n2".into(),
                    label: gl("Order"),
                    description: LocalizedText::default(),
                    properties: vec![PropertyDef {
                        id: "p3".into(),
                        name: pk("total"),
                        property_type: PropertyType::Float,
                        nullable: false,
                        default_value: None,
                        description: LocalizedText::default(),
                        classification: None,
                        ..Default::default()
                    }],
                    constraints: vec![],
                    ..Default::default()
                },
            ],
            vec![EdgeTypeDef {
                id: "e1".into(),
                label: gl("PLACED"),
                description: LocalizedText::default(),
                source_node_id: "n1".into(),
                target_node_id: "n2".into(),
                properties: vec![],
                cardinality: Cardinality::OneToMany,
                ..Default::default()
            }],
            vec![],
        )
    }

    #[test]
    fn emits_type_and_field_definitions() {
        let schema = generate_graphql(&sample_ontology());
        assert!(schema.contains("type Customer {"));
        assert!(schema.contains("type Order {"));
        // Unique-constrained field becomes ID!
        assert!(schema.contains("id: ID!"));
        // Nullable string with description
        assert!(schema.contains("email: String"));
    }

    #[test]
    fn emits_relationship_field_with_directive() {
        let schema = generate_graphql(&sample_ontology());
        // Customer side: outgoing OneToMany list
        assert!(schema.contains("@relationship(type: \"PLACED\", direction: OUT)"));
        // Order side: incoming
        assert!(schema.contains("@relationship(type: \"PLACED\", direction: IN)"));
    }

    #[test]
    fn deprecated_property_emits_deprecated_directive_with_replacement() {
        let mut ontology = sample_ontology();
        ontology
            .update_node_type(&"n1".into(), |n| {
                n.properties[1].deprecated_at = Some(chrono::Utc::now());
                n.properties[1].replaced_by_id = Some("p1".into());
            })
            .unwrap();
        let schema = generate_graphql(&ontology);
        assert!(
            schema.contains("@deprecated(reason: \"Replaced by `id`\")"),
            "expected @deprecated with replacement reason: {schema}"
        );
    }

    #[test]
    fn deprecated_edge_marks_relationship_field() {
        let mut ontology = sample_ontology();
        ontology
            .update_edge_type(&"e1".into(), |e| {
                e.deprecated_at = Some(chrono::Utc::now());
            })
            .unwrap();
        let schema = generate_graphql(&ontology);
        // Expect every relationship field for this edge to carry @deprecated.
        let count = schema
            .matches("@deprecated(reason: \"Deprecated\")")
            .count();
        assert!(
            count >= 2,
            "expected @deprecated on both directions of the edge: {schema}"
        );
    }

    #[test]
    fn target_role_becomes_outgoing_field_name() {
        let mut ontology = sample_ontology();
        ontology
            .update_edge_type(&"e1".into(), |e| {
                e.target_role = Some("orders".into());
                e.source_role = Some("customer".into());
            })
            .unwrap();
        let schema = generate_graphql(&ontology);
        // Outgoing field on Customer should use the target role.
        assert!(
            schema.contains("orders: [Order!]!"),
            "outgoing field should use target_role `orders`: {schema}"
        );
        // Incoming field on Order should use the source role. Edge is
        // OneToMany (one Customer → many Orders), so Order sees a single
        // Customer field, not a list.
        assert!(
            schema.contains("customer: Customer!"),
            "incoming field should use source_role `customer`: {schema}"
        );
    }

    #[test]
    fn deprecated_node_marks_type_docstring() {
        let mut ontology = sample_ontology();
        ontology
            .update_node_type(&"n1".into(), |n| {
                n.deprecated_at = Some(chrono::Utc::now());
            })
            .unwrap();
        let schema = generate_graphql(&ontology);
        // GraphQL doesn't allow @deprecated on object types, so fall through
        // to a docstring marker that consumers can grep for.
        assert!(
            schema.contains("\"\"\"[DEPRECATED] A buyer\"\"\""),
            "expected [DEPRECATED] marker in type docstring: {schema}"
        );
    }
}
