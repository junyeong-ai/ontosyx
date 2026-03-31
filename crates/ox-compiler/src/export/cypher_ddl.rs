use ox_core::ontology_ir::{IndexDef, NodeConstraint, NodeTypeDef, OntologyIR, VectorSimilarity};
use ox_core::types::PropertyType;

/// Generate Cypher DDL (constraints, indexes, structure comments) from an OntologyIR.
pub fn generate_cypher_ddl(ontology: &OntologyIR) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push(format!("// Cypher DDL for: {}", ontology.name));
    if let Some(desc) = &ontology.description {
        lines.push(format!("// {desc}"));
    }
    lines.push(format!("// Version: {}", ontology.version));
    lines.push(String::new());

    // Node constraints
    lines.push("// --- Node Constraints ---".to_string());
    for node in &ontology.node_types {
        for cdef in &node.constraints {
            match &cdef.constraint {
                NodeConstraint::Unique { property_ids } => {
                    let props = resolve_prop_names(node, property_ids);
                    lines.push(format!(
                        "CREATE CONSTRAINT IF NOT EXISTS FOR (n:{}) REQUIRE ({}) IS UNIQUE;",
                        escape_cypher_label(&node.label),
                        props
                            .iter()
                            .map(|p| format!("n.{}", escape_cypher_label(p)))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                NodeConstraint::Exists { property_id } => {
                    if let Some(name) = resolve_prop_name(node, property_id) {
                        lines.push(format!(
                            "CREATE CONSTRAINT IF NOT EXISTS FOR (n:{}) REQUIRE n.{} IS NOT NULL;",
                            escape_cypher_label(&node.label),
                            escape_cypher_label(&name)
                        ));
                    }
                }
                NodeConstraint::NodeKey { property_ids } => {
                    let props = resolve_prop_names(node, property_ids);
                    lines.push(format!(
                        "CREATE CONSTRAINT IF NOT EXISTS FOR (n:{}) REQUIRE ({}) IS NODE KEY;",
                        escape_cypher_label(&node.label),
                        props
                            .iter()
                            .map(|p| format!("n.{}", escape_cypher_label(p)))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            }
        }
    }
    lines.push(String::new());

    // Indexes
    lines.push("// --- Indexes ---".to_string());
    for index in &ontology.indexes {
        match index {
            IndexDef::Single {
                node_id,
                property_id,
                ..
            } => {
                if let (Some(label), Some(prop)) = (
                    ontology.node_label(node_id),
                    ontology
                        .node_by_id(node_id)
                        .and_then(|n| n.properties.iter().find(|p| p.id == *property_id)),
                ) {
                    lines.push(format!(
                        "CREATE INDEX IF NOT EXISTS FOR (n:{}) ON (n.{});",
                        escape_cypher_label(label),
                        escape_cypher_label(&prop.name)
                    ));
                }
            }
            IndexDef::Composite {
                node_id,
                property_ids,
                ..
            } => {
                if let Some(label) = ontology.node_label(node_id) {
                    let node = ontology.node_by_id(node_id);
                    let props: Vec<String> = property_ids
                        .iter()
                        .filter_map(|pid| {
                            node.and_then(|n| n.properties.iter().find(|p| p.id == *pid))
                                .map(|p| format!("n.{}", escape_cypher_label(&p.name)))
                        })
                        .collect();
                    lines.push(format!(
                        "CREATE INDEX IF NOT EXISTS FOR (n:{}) ON ({});",
                        escape_cypher_label(label),
                        props.join(", ")
                    ));
                }
            }
            IndexDef::FullText {
                name,
                node_id,
                property_ids,
                ..
            } => {
                if let Some(label) = ontology.node_label(node_id) {
                    let node = ontology.node_by_id(node_id);
                    let props: Vec<String> = property_ids
                        .iter()
                        .filter_map(|pid| {
                            node.and_then(|n| n.properties.iter().find(|p| p.id == *pid))
                                .map(|p| format!("n.{}", escape_cypher_label(&p.name)))
                        })
                        .collect();
                    lines.push(format!(
                        "CREATE FULLTEXT INDEX {} IF NOT EXISTS FOR (n:{}) ON EACH [{}];",
                        escape_cypher_label(name),
                        escape_cypher_label(label),
                        props.join(", ")
                    ));
                }
            }
            IndexDef::Vector {
                node_id,
                property_id,
                dimensions,
                similarity,
                ..
            } => {
                if let (Some(label), Some(prop)) = (
                    ontology.node_label(node_id),
                    ontology
                        .node_by_id(node_id)
                        .and_then(|n| n.properties.iter().find(|p| p.id == *property_id)),
                ) {
                    let sim = match similarity {
                        VectorSimilarity::Cosine => "cosine",
                        VectorSimilarity::Euclidean => "euclidean",
                    };
                    lines.push(format!(
                        "CREATE VECTOR INDEX IF NOT EXISTS FOR (n:{}) ON (n.{}) OPTIONS {{indexConfig: {{`vector.dimensions`: {dimensions}, `vector.similarity_function`: '{sim}'}}}};",
                        escape_cypher_label(label),
                        escape_cypher_label(&prop.name)
                    ));
                }
            }
        }
    }
    lines.push(String::new());

    // Node structure comments
    lines.push("// --- Node Structures ---".to_string());
    for node in &ontology.node_types {
        lines.push(format!("// :{}", node.label));
        for prop in &node.properties {
            let nullable = if prop.nullable { " (nullable)" } else { "" };
            lines.push(format!(
                "//   .{}: {}{}",
                prop.name,
                cypher_type(&prop.property_type),
                nullable,
            ));
        }
    }
    lines.push(String::new());

    // Edge structure comments
    lines.push("// --- Relationship Structures ---".to_string());
    for edge in &ontology.edge_types {
        let src = ontology.node_label(&edge.source_node_id).unwrap_or("?");
        let tgt = ontology.node_label(&edge.target_node_id).unwrap_or("?");
        lines.push(format!("// (:{src})-[:{}]->(:{tgt})", edge.label));
        for prop in &edge.properties {
            lines.push(format!(
                "//   .{}: {}",
                prop.name,
                cypher_type(&prop.property_type),
            ));
        }
    }

    lines.join("\n")
}

fn cypher_type(pt: &PropertyType) -> &'static str {
    match pt {
        PropertyType::Bool => "BOOLEAN",
        PropertyType::Int => "INTEGER",
        PropertyType::Float => "FLOAT",
        PropertyType::String => "STRING",
        PropertyType::Date => "DATE",
        PropertyType::DateTime => "DATETIME",
        PropertyType::Duration => "DURATION",
        PropertyType::Bytes => "STRING",
        PropertyType::List { .. } => "LIST<STRING>",
        PropertyType::Map => "MAP",
    }
}

fn escape_cypher_label(name: &str) -> String {
    ox_core::types::escape_cypher_identifier(name)
}

fn resolve_prop_name(node: &NodeTypeDef, prop_id: &str) -> Option<String> {
    node.properties
        .iter()
        .find(|p| p.id == prop_id)
        .map(|p| p.name.clone())
}

fn resolve_prop_names(
    node: &NodeTypeDef,
    prop_ids: &[ox_core::ontology_ir::PropertyId],
) -> Vec<String> {
    prop_ids
        .iter()
        .filter_map(|pid| {
            node.properties
                .iter()
                .find(|p| p.id == *pid)
                .map(|p| p.name.clone())
        })
        .collect()
}
