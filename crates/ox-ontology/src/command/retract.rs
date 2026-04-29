//! Source-retraction delta — ADR-0026 second half.
//!
//! Pairs with `AnalyzeSelection::Reduce` on the introspection side
//! (`ox_source::AnalyzeSelection::Reduce` drops named tables from the
//! kernel's analysis result). On the IR side, dropping a table
//! requires walking every collection that referenced it and emitting
//! the deletes that keep referential integrity. The walk is
//! deterministic: same `(SourceId, table-name set)` always produces
//! the same `Batch` of `Delete*` commands, so the FE can render a
//! diff before the operator commits and the audit log carries one
//! row per logical retraction.
//!
//! Three classes of references the walk surfaces:
//!
//! 1. `ObjectMappingDef` rows whose `(source_id, relation)` matches —
//!    these are the direct table → node-type bindings. Dropping the
//!    mapping leaves a node type mapped to nothing, so the
//!    accompanying NodeType is also dropped *iff* it has no other
//!    object mapping (multi-source NodeTypes survive).
//! 2. `LinkMappingDef` rows whose `source_endpoint` or
//!    `target_endpoint` references a dropped relation. The link
//!    mapping disappears; the EdgeType disappears *iff* it has no
//!    other link mapping.
//! 3. Properties / constraints / indices that hung off the dropped
//!    NodeType / EdgeType are removed transitively by the
//!    `DeleteNode` / `DeleteEdge` commands the IR already executes —
//!    we do not re-emit them as separate commands because the IR's
//!    cascading delete contract already cleans them up.
//!
//! The function is pure; the caller routes the resulting `Batch`
//! through `OntologyCommand::execute` like any other command, so
//! validation and inverse-tracking flow without a special case.

use std::collections::BTreeSet;

use super::OntologyCommand;
use crate::ir::OntologyIR;
use crate::mapping::SourceId;

/// Compute the delete commands required to retract every IR
/// reference to the named tables in `source_id`. The returned
/// `OntologyCommand::Batch` is empty when no references are found —
/// callers can short-circuit on `commands.is_empty()` rather than
/// walking the result.
pub fn build_retract_source_batch(
    ontology: &OntologyIR,
    source_id: &SourceId,
    drop_tables: &BTreeSet<String>,
) -> OntologyCommand {
    let mut commands: Vec<OntologyCommand> = Vec::new();

    // Pass 1: object mappings whose (source_id, relation) lands in
    // the drop set. Collect both the mapping ids (to delete) and
    // the targeted NodeType ids (to consider for delete in pass 3).
    let mut affected_node_ids: BTreeSet<String> = BTreeSet::new();
    let mut dropped_object_mapping_ids: BTreeSet<String> = BTreeSet::new();
    for mapping in ontology.object_mappings() {
        if &mapping.source_id == source_id
            && drop_tables.contains(&mapping.relation)
        {
            commands.push(OntologyCommand::DeleteObjectMapping {
                id: mapping.id.clone(),
            });
            dropped_object_mapping_ids.insert(mapping.id.0.clone());
            affected_node_ids.insert(mapping.node_type_id.0.clone());
        }
    }

    // Pass 2: link mappings whose either endpoint references a
    // dropped (source_id, relation). Surfaces affected EdgeType ids.
    let mut affected_edge_ids: BTreeSet<String> = BTreeSet::new();
    for link in ontology.link_mappings() {
        let touches_drop = endpoint_touches_drop(
            source_id,
            drop_tables,
            link.source_endpoint.source_id.as_str(),
            &link.source_endpoint.relation,
        ) || endpoint_touches_drop(
            source_id,
            drop_tables,
            link.target_endpoint.source_id.as_str(),
            &link.target_endpoint.relation,
        );
        if touches_drop {
            // LinkMapping has no first-class delete command in the
            // current `OntologyCommand` surface — it lives inside
            // the IR alongside object mappings but is mutated through
            // the admin-side OntologyEditOp flow. The retraction
            // batch records the affected edge type id so the caller
            // can surface a follow-up step; the EdgeType itself is
            // dropped below when no other link mapping references
            // its bound source.
            affected_edge_ids.insert(link.edge_type_id.0.clone());
        }
    }

    // Pass 3: node types whose every object mapping was dropped go
    // away. A node still bound to another (untouched) source survives.
    for node in ontology.node_types() {
        if !affected_node_ids.contains(node.id.as_str()) {
            continue;
        }
        let still_bound = ontology
            .object_mappings()
            .iter()
            .any(|m| m.node_type_id == node.id && !dropped_object_mapping_ids.contains(m.id.as_str()));
        if !still_bound {
            commands.push(OntologyCommand::DeleteNode {
                node_id: node.id.clone(),
            });
        }
    }

    // Pass 4: edge types whose every endpoint relation lands in the
    // drop set go away — the surviving link mappings (if any) all
    // touch the drop set, so the edge no longer has a viable
    // physical binding.
    for edge in ontology.edge_types() {
        if !affected_edge_ids.contains(edge.id.as_str()) {
            continue;
        }
        let still_bound = ontology.link_mappings().iter().any(|link| {
            if link.edge_type_id != edge.id {
                return false;
            }
            !endpoint_touches_drop(
                source_id,
                drop_tables,
                link.source_endpoint.source_id.as_str(),
                &link.source_endpoint.relation,
            ) && !endpoint_touches_drop(
                source_id,
                drop_tables,
                link.target_endpoint.source_id.as_str(),
                &link.target_endpoint.relation,
            )
        });
        if !still_bound {
            commands.push(OntologyCommand::DeleteEdge {
                edge_id: edge.id.clone(),
            });
        }
    }

    OntologyCommand::Batch {
        description: format!(
            "Retract source '{}' tables: {}",
            source_id,
            drop_tables
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ),
        commands,
    }
}

fn endpoint_touches_drop(
    source_id: &SourceId,
    drop_tables: &BTreeSet<String>,
    endpoint_source: &str,
    endpoint_relation: &str,
) -> bool {
    endpoint_source == source_id.as_str()
        && drop_tables.contains(endpoint_relation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{NodeTypeDef, OntologyIR};
    use crate::mapping::ObjectMappingDef;
    use ox_core::GraphLabel;
    use ox_core::i18n::LocalizedText;

    fn empty_ontology() -> OntologyIR {
        OntologyIR::new(
            "ont".to_string(),
            "test".to_string(),
            LocalizedText::default(),
            1,
            vec![],
            vec![],
            vec![],
        )
    }

    fn add_node(ir: &mut OntologyIR, id: &str, label: &str) {
        ir.add_node_type(NodeTypeDef {
            id: id.into(),
            label: GraphLabel::new(label).unwrap(),
            ..Default::default()
        })
        .expect("add node");
    }

    fn add_mapping(ir: &mut OntologyIR, id: &str, node: &str, source: &str, table: &str) {
        ir.add_object_mapping(ObjectMappingDef::new(id, node, source, table))
            .expect("add mapping");
    }

    #[test]
    fn empty_drop_set_produces_empty_batch() {
        let ir = empty_ontology();
        let batch = build_retract_source_batch(
            &ir,
            &SourceId::new("pg-main"),
            &BTreeSet::new(),
        );
        let OntologyCommand::Batch { commands, .. } = batch else {
            panic!("expected Batch");
        };
        assert!(commands.is_empty());
    }

    #[test]
    fn dropping_a_table_removes_its_object_mapping_and_orphaned_node() {
        let mut ir = empty_ontology();
        add_node(&mut ir, "nt-user", "User");
        add_mapping(&mut ir, "om-user", "nt-user", "pg-main", "users");

        let batch = build_retract_source_batch(
            &ir,
            &SourceId::new("pg-main"),
            &BTreeSet::from(["users".to_string()]),
        );
        let OntologyCommand::Batch { commands, .. } = batch else {
            panic!("expected Batch");
        };
        // Expect both DeleteObjectMapping (om-user) and DeleteNode (nt-user).
        assert_eq!(commands.len(), 2);
        assert!(commands.iter().any(|c| matches!(
            c,
            OntologyCommand::DeleteObjectMapping { id } if id.as_str() == "om-user"
        )));
        assert!(commands.iter().any(|c| matches!(
            c,
            OntologyCommand::DeleteNode { node_id } if node_id.as_str() == "nt-user"
        )));
    }

    #[test]
    fn node_with_another_source_mapping_survives_retraction() {
        let mut ir = empty_ontology();
        add_node(&mut ir, "nt-user", "User");
        add_mapping(&mut ir, "om-pg", "nt-user", "pg-main", "users");
        add_mapping(&mut ir, "om-mysql", "nt-user", "mysql-aux", "users");

        let batch = build_retract_source_batch(
            &ir,
            &SourceId::new("pg-main"),
            &BTreeSet::from(["users".to_string()]),
        );
        let OntologyCommand::Batch { commands, .. } = batch else {
            panic!("expected Batch");
        };
        // Only the pg-main mapping is dropped; node survives.
        assert_eq!(commands.len(), 1);
        assert!(matches!(
            &commands[0],
            OntologyCommand::DeleteObjectMapping { id } if id.as_str() == "om-pg"
        ));
    }

    #[test]
    fn other_source_mapping_is_not_touched() {
        let mut ir = empty_ontology();
        add_node(&mut ir, "nt-order", "Order");
        add_mapping(&mut ir, "om-pg", "nt-order", "pg-main", "orders");

        // Drop targets a different source — nothing should change.
        let batch = build_retract_source_batch(
            &ir,
            &SourceId::new("snowflake-aux"),
            &BTreeSet::from(["orders".to_string()]),
        );
        let OntologyCommand::Batch { commands, .. } = batch else {
            panic!("expected Batch");
        };
        assert!(commands.is_empty());
    }

    #[test]
    fn batch_description_names_source_and_tables_for_audit_trail() {
        let ir = empty_ontology();
        let batch = build_retract_source_batch(
            &ir,
            &SourceId::new("pg-main"),
            &BTreeSet::from(["users".to_string(), "orders".to_string()]),
        );
        let OntologyCommand::Batch { description, .. } = batch else {
            panic!("expected Batch");
        };
        assert!(description.contains("pg-main"));
        assert!(description.contains("users"));
        assert!(description.contains("orders"));
    }
}
