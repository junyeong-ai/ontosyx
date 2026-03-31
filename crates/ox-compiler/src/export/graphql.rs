use ox_core::ontology_ir::{Cardinality, NodeConstraint, NodeTypeDef, OntologyIR};
use ox_core::types::PropertyType;

/// Generate a GraphQL schema from an OntologyIR.
pub fn generate_graphql(ontology: &OntologyIR) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push(format!("# GraphQL Schema for: {}", ontology.name));
    if let Some(desc) = &ontology.description {
        lines.push(format!("# {desc}"));
    }
    lines.push(String::new());

    for node in &ontology.node_types {
        // Determine if any property is a PK (unique constraint -> ID type)
        let pk_name = find_pk_property(node);

        if let Some(desc) = &node.description {
            lines.push(format!("\"\"\"{}\"\"\"", desc));
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
            let desc_comment = prop
                .description
                .as_ref()
                .map(|d| format!("  # {d}"))
                .unwrap_or_default();
            lines.push(format!(
                "  {}: {}{}",
                graphql_safe_name(&prop.name),
                gql_type,
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
    if nullable {
        base
    } else {
        format!("{base}!")
    }
}

fn graphql_safe_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    // GraphQL names must start with a letter or underscore
    if sanitized
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit())
    {
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
    for edge in &ontology.edge_types {
        // Outgoing edges (source = this node)
        if edge.source_node_id == node.id
            && let Some(target) = ontology.node_by_id(&edge.target_node_id)
        {
            let field_name = relationship_field_name(&edge.label, &target.label, true);
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
                "  {field_name}: {gql_type}  @relationship(type: \"{}\", direction: OUT)",
                edge.label
            ));
        }
        // Incoming edges (target = this node)
        if edge.target_node_id == node.id
            && let Some(source) = ontology.node_by_id(&edge.source_node_id)
        {
            let field_name = relationship_field_name(&edge.label, &source.label, false);
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
                "  {field_name}: {gql_type}  @relationship(type: \"{}\", direction: IN)",
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
