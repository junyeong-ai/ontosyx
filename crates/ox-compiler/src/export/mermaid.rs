use std::collections::BTreeMap;

use ox_core::ontology_ir::{Cardinality, NodeConstraint, NodeTypeDef, OntologyIR};
use ox_core::types::PropertyType;

/// Generate a Mermaid ER diagram from an OntologyIR.
///
/// Mermaid's `erDiagram` syntax does not accept `classDef` styling on
/// entities (that's a `flowchart` / `classDiagram` feature), so the
/// `tags` field is surfaced two ways instead:
///
/// 1. Each entity / relationship gets a `%% tags: …` comment immediately
///    above its definition, so a reader scanning the source sees the
///    classification inline.
/// 2. A trailing `%% Tag legend` block groups entities and edges by tag
///    so the reader can see at a glance which nodes belong to "core",
///    "finance", "deprecated", etc.
///
/// Deprecation also surfaces (`%% deprecated` markers) since Mermaid ER
/// has no direct annotation for it either.
pub fn generate_mermaid(ontology: &OntologyIR) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push(format!("%% {}", ontology.name));
    lines.push("erDiagram".to_string());

    // Track tag → entities for the trailing legend block.
    let mut tag_index: BTreeMap<String, Vec<String>> = BTreeMap::new();

    // Entity definitions
    for node in ontology.node_types() {
        let pk_name = find_pk_property(node);
        let id = mermaid_id(&node.label);

        // Node-level tags live under `governance.tags`; the direct
        // `tags` field is only present on edge types.
        let node_tags: &[String] = node
            .governance
            .as_ref()
            .map(|g| g.tags.as_slice())
            .unwrap_or(&[]);

        // Inline tag / deprecation comment above the entity.
        push_tag_comment(
            &mut lines,
            "    ",
            node_tags,
            node.deprecated_at.is_some(),
        );
        for tag in node_tags {
            tag_index
                .entry(tag.clone())
                .or_default()
                .push(format!("entity {id}"));
        }

        lines.push(format!("    {id} {{"));
        for prop in &node.properties {
            let pk_marker = if Some(prop.name.as_str()) == pk_name {
                " PK"
            } else {
                ""
            };
            lines.push(format!(
                "        {} {}{}",
                mermaid_type(&prop.property_type),
                mermaid_id(&prop.name),
                pk_marker,
            ));
        }
        lines.push("    }".to_string());
    }

    // Relationships
    for edge in ontology.edge_types() {
        let src = ontology
            .node_label(&edge.source_node_id)
            .map(mermaid_id)
            .unwrap_or_else(|| "UNKNOWN".to_string());
        let tgt = ontology
            .node_label(&edge.target_node_id)
            .map(mermaid_id)
            .unwrap_or_else(|| "UNKNOWN".to_string());
        let cardinality = mermaid_cardinality(&edge.cardinality);

        push_tag_comment(&mut lines, "    ", &edge.tags, edge.deprecated_at.is_some());
        for tag in &edge.tags {
            tag_index
                .entry(tag.clone())
                .or_default()
                .push(format!("edge {src}->{tgt} ({})", edge.label));
        }

        // Edge label carries optional endpoint roles so the reader sees
        // "PLACED (buyer→order)" — the directional role is often the piece
        // that makes an otherwise cryptic label meaningful.
        let role_suffix = edge_role_suffix(edge);
        lines.push(format!(
            "    {src} {cardinality} {tgt} : \"{}{role_suffix}\"",
            edge.label
        ));
    }

    if !tag_index.is_empty() {
        lines.push(String::new());
        lines.push("%% Tag legend".into());
        for (tag, members) in &tag_index {
            lines.push(format!("%%   {tag}: {}", members.join(", ")));
        }
    }

    lines.join("\n")
}

/// Emit a `%% tags: …` comment above an entity / relationship if either
/// tags or deprecation are present. Skipped entirely when both are
/// empty so the existing diagram output is unchanged.
fn push_tag_comment(lines: &mut Vec<String>, indent: &str, tags: &[String], deprecated: bool) {
    if tags.is_empty() && !deprecated {
        return;
    }
    let mut parts: Vec<String> = Vec::new();
    if deprecated {
        parts.push("deprecated".into());
    }
    parts.extend(tags.iter().cloned());
    lines.push(format!("{indent}%% tags: [{}]", parts.join(", ")));
}

/// Return `" (src→tgt)"` when at least one endpoint role is present,
/// empty string otherwise. Prefers the explicit role; falls back to `?`
/// so the reader sees exactly which side is unnamed.
fn edge_role_suffix(edge: &ox_core::ontology_ir::EdgeTypeDef) -> String {
    if edge.source_role.is_none() && edge.target_role.is_none() {
        return String::new();
    }
    let src = edge.source_role.as_deref().unwrap_or("?");
    let tgt = edge.target_role.as_deref().unwrap_or("?");
    format!(" ({src}→{tgt})")
}

fn mermaid_type(pt: &PropertyType) -> &'static str {
    match pt {
        PropertyType::Bool => "boolean",
        PropertyType::Int => "int",
        PropertyType::Float => "float",
        PropertyType::String => "string",
        PropertyType::Date => "date",
        PropertyType::DateTime => "datetime",
        PropertyType::Duration => "string",
        PropertyType::Bytes => "bytes",
        PropertyType::List { .. } => "list",
        PropertyType::Map => "map",
    }
}

/// Sanitize a label for Mermaid: replace spaces/special chars with underscores.
fn mermaid_id(label: &str) -> String {
    label
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
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

fn mermaid_cardinality(c: &Cardinality) -> &'static str {
    match c {
        Cardinality::OneToOne => "||--||",
        Cardinality::OneToMany => "||--|{",
        Cardinality::ManyToOne => "}|--||",
        Cardinality::ManyToMany => "}|--|{",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::LocalizedText;
    use ox_core::ontology_ir::{
        Cardinality, EdgeTypeDef, Governance, NodeTypeDef, PropertyDef,
    };

    fn sample_ontology() -> OntologyIR {
        OntologyIR::new(
            "m-test".into(),
            "Sales".into(),
            LocalizedText::default(),
            1,
            vec![
                NodeTypeDef {
                    id: "n1".into(),
                    label: "Customer".into(),
                    description: LocalizedText::default(),
                    properties: vec![PropertyDef {
                        id: "p1".into(),
                        name: "email".into(),
                        property_type: PropertyType::String,
                        nullable: false,
                        default_value: None,
                        description: LocalizedText::default(),
                        classification: None,
                        ..Default::default()
                    }],
                    constraints: vec![],
                    governance: Some(Governance {
                        tags: vec!["core".into(), "crm".into()],
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                NodeTypeDef {
                    id: "n2".into(),
                    label: "Order".into(),
                    description: LocalizedText::default(),
                    properties: vec![PropertyDef {
                        id: "p2".into(),
                        name: "total".into(),
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
                label: "PLACED".into(),
                description: LocalizedText::default(),
                source_node_id: "n1".into(),
                target_node_id: "n2".into(),
                properties: vec![],
                cardinality: Cardinality::OneToMany,
                tags: vec!["core".into()],
                ..Default::default()
            }],
            vec![],
        )
    }

    #[test]
    fn emits_er_diagram_structure() {
        let out = generate_mermaid(&sample_ontology());
        assert!(out.starts_with("%% Sales"), "header comment first");
        assert!(out.contains("erDiagram"));
        assert!(out.contains("Customer {"));
        assert!(out.contains("Order {"));
        assert!(out.contains("Customer ||--|{ Order : \"PLACED\""));
    }

    #[test]
    fn tags_surface_as_inline_comments_and_legend() {
        let out = generate_mermaid(&sample_ontology());
        // Inline comment above the entity carries the tags.
        assert!(
            out.contains("%% tags: [core, crm]"),
            "node tag comment missing: {out}"
        );
        // Edge also gets a tag comment.
        assert!(
            out.contains("%% tags: [core]"),
            "edge tag comment missing: {out}"
        );
        // Legend block enumerates tags at the bottom.
        assert!(out.contains("%% Tag legend"));
        assert!(out.contains("%%   core:"));
        assert!(out.contains("entity Customer"));
        assert!(out.contains("edge Customer->Order (PLACED)"));
    }

    #[test]
    fn edge_roles_flow_into_edge_label() {
        let mut ontology = sample_ontology();
        ontology
            .update_edge_type(&"e1".into(), |e| {
                e.source_role = Some("buyer".into());
                e.target_role = Some("purchase".into());
            })
            .unwrap();
        let out = generate_mermaid(&ontology);
        assert!(
            out.contains("PLACED (buyer→purchase)"),
            "edge label should carry role suffix: {out}"
        );
    }

    #[test]
    fn deprecation_surfaces_in_tag_comment() {
        let mut ontology = sample_ontology();
        ontology
            .update_node_type(&"n2".into(), |n| {
                n.deprecated_at = Some(chrono::Utc::now());
            })
            .unwrap();
        let out = generate_mermaid(&ontology);
        assert!(
            out.contains("%% tags: [deprecated]"),
            "deprecated node should carry marker comment: {out}"
        );
    }

    #[test]
    fn no_comment_when_entity_has_no_tags_or_deprecation() {
        let ontology = OntologyIR::new(
            "plain".into(),
            "Plain".into(),
            LocalizedText::default(),
            1,
            vec![NodeTypeDef {
                id: "n1".into(),
                label: "Plain".into(),
                description: LocalizedText::default(),
                properties: vec![],
                constraints: vec![],
                ..Default::default()
            }],
            vec![],
            vec![],
        );
        let out = generate_mermaid(&ontology);
        assert!(!out.contains("%% tags:"), "no tags comment expected: {out}");
        assert!(
            !out.contains("%% Tag legend"),
            "no legend when tag index is empty: {out}"
        );
    }
}
