use ox_core::ontology_diff::*;
use ox_core::ontology_ir::OntologyIR;

use super::params::escape_identifier;
use super::schema::compile_node_constraints;

// ---------------------------------------------------------------------------
// MigrationPlan — forward + rollback DDL for an ontology diff
// ---------------------------------------------------------------------------

/// A single data-level migration step (MATCH/SET/REMOVE, not DDL).
#[derive(Debug, Clone, serde::Serialize)]
pub struct DataMigrationStep {
    /// Human-readable description of what this step does
    pub description: String,
    /// The Cypher statement to execute
    pub cypher: String,
    /// Whether this step may lose data or fail on existing rows
    pub is_destructive: bool,
}

/// A migration plan generated from an OntologyDiff.
/// Contains forward (up) and rollback (down) DDL statements.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MigrationPlan {
    /// Forward DDL statements to apply the migration
    pub up: Vec<String>,
    /// Rollback DDL statements to undo the migration
    pub down: Vec<String>,
    /// Non-breaking concerns that may need attention
    pub warnings: Vec<String>,
    /// Destructive changes that require explicit confirmation
    pub breaking_changes: Vec<String>,
    /// Data-level migration steps (property backfill, label renames, etc.)
    pub data_migrations: Vec<DataMigrationStep>,
}

/// Compile a migration plan from an OntologyDiff.
///
/// Uses the existing `compile_node_constraints` for constraint generation
/// and produces DROP/CREATE statements for schema evolution.
pub fn compile_migration(
    diff: &OntologyDiff,
    old: &OntologyIR,
    new: &OntologyIR,
) -> MigrationPlan {
    let mut up = Vec::new();
    let mut down = Vec::new();
    let mut warnings = Vec::new();
    let mut breaking_changes = Vec::new();

    // --- Added nodes: create constraints + indexes ---
    for node in &diff.added_nodes {
        let constraint_stmts = compile_node_constraints(node);
        up.extend(constraint_stmts.clone());
        // Rollback: drop the constraints
        for stmt in &constraint_stmts {
            if let Some(down_stmt) = reverse_index_stmt(stmt) {
                down.push(down_stmt);
            }
        }
        // Auto-create range indexes for required properties
        for prop in &node.properties {
            if !prop.nullable {
                let label = escape_identifier(&node.label);
                let prop_name = escape_identifier(&prop.name);
                up.push(format!(
                    "CREATE INDEX IF NOT EXISTS FOR (n:{label}) ON (n.{prop_name})"
                ));
                down.push(format!(
                    "DROP INDEX IF EXISTS index_{}_{}",
                    node.label.to_lowercase(),
                    prop.name.to_lowercase()
                ));
            }
        }
    }

    // --- Removed nodes: drop constraints (BREAKING) ---
    for node in &diff.removed_nodes {
        breaking_changes.push(format!(
            "Node type '{}' will be removed — all existing nodes of this type should be migrated or deleted first",
            node.label
        ));
        let constraint_stmts = compile_node_constraints(node);
        // Forward: drop the constraints
        for stmt in &constraint_stmts {
            if let Some(drop_stmt) = reverse_index_stmt(stmt) {
                up.push(drop_stmt);
            }
        }
        // Rollback: recreate the constraints
        down.extend(constraint_stmts);
    }

    // --- Modified nodes ---
    for node_diff in &diff.modified_nodes {
        let new_node = new.node_types.iter().find(|n| n.id == node_diff.node_id);
        let old_node = old.node_types.iter().find(|n| n.id == node_diff.node_id);

        for change in &node_diff.changes {
            match change {
                NodeChange::LabelChanged { old: old_label, new: new_label } => {
                    warnings.push(format!(
                        "Node label rename '{}' → '{}' — requires data migration (MATCH (n:{}) SET n:{} REMOVE n:{})",
                        old_label, new_label,
                        escape_identifier(old_label),
                        escape_identifier(new_label),
                        escape_identifier(old_label),
                    ));
                    // Recreate constraints under new label
                    if let Some(node) = new_node {
                        let stmts = compile_node_constraints(node);
                        up.extend(stmts);
                    }
                    // Drop old label constraints
                    if let Some(node) = old_node {
                        let stmts = compile_node_constraints(node);
                        for stmt in &stmts {
                            if let Some(drop_stmt) = reverse_index_stmt(stmt) {
                                up.push(drop_stmt);
                            }
                        }
                    }
                }
                NodeChange::PropertyAdded { property } => {
                    if !property.nullable {
                        breaking_changes.push(format!(
                            "Required property '{}' added to '{}' — existing nodes will need this value set",
                            property.name, node_diff.label
                        ));
                    }
                    // Create index for non-nullable added properties
                    if !property.nullable {
                        let label = escape_identifier(&node_diff.label);
                        let prop_name = escape_identifier(&property.name);
                        up.push(format!(
                            "CREATE INDEX IF NOT EXISTS FOR (n:{label}) ON (n.{prop_name})"
                        ));
                    }
                }
                NodeChange::PropertyRemoved { property } => {
                    warnings.push(format!(
                        "Property '{}' removed from '{}' — existing data will be orphaned",
                        property.name, node_diff.label
                    ));
                }
                NodeChange::PropertyModified { property_name, changes } => {
                    for pc in changes {
                        if let PropertyChange::TypeChanged { old, new } = pc {
                            breaking_changes.push(format!(
                                "Property '{}' on '{}' type changed: {} → {} — data migration required",
                                property_name, node_diff.label, old, new
                            ));
                        }
                    }
                }
                NodeChange::ConstraintAdded { constraint } => {
                    // Find the new node to compile its constraints
                    if let Some(node) = new_node {
                        let stmts = compile_node_constraints(node);
                        up.extend(stmts);
                    } else {
                        warnings.push(format!(
                            "Constraint added to '{}': {} (could not compile)",
                            node_diff.label, constraint
                        ));
                    }
                }
                NodeChange::ConstraintRemoved { constraint } => {
                    warnings.push(format!(
                        "Constraint removed from '{}': {}",
                        node_diff.label, constraint
                    ));
                    if let Some(node) = old_node {
                        let stmts = compile_node_constraints(node);
                        for stmt in &stmts {
                            if let Some(drop_stmt) = reverse_index_stmt(stmt) {
                                up.push(drop_stmt);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // --- Added edges ---
    for edge in &diff.added_edges {
        let src_label = new
            .node_label(&edge.source_node_id)
            .unwrap_or("UNKNOWN");
        let tgt_label = new
            .node_label(&edge.target_node_id)
            .unwrap_or("UNKNOWN");
        warnings.push(format!(
            "New edge type '{}' ({}→{}) — no schema DDL needed for edges in Neo4j, but ensure endpoints exist",
            edge.label, src_label, tgt_label
        ));
    }

    // --- Removed edges ---
    for edge in &diff.removed_edges {
        let src_label = old
            .node_label(&edge.source_node_id)
            .unwrap_or("UNKNOWN");
        let tgt_label = old
            .node_label(&edge.target_node_id)
            .unwrap_or("UNKNOWN");
        breaking_changes.push(format!(
            "Edge type '{}' ({}→{}) removed — existing relationships should be deleted",
            edge.label, src_label, tgt_label
        ));
    }

    // Deduplicate statements (sort-based to catch non-consecutive duplicates)
    up.sort();
    up.dedup();
    down.sort();
    down.dedup();

    let data_migrations = compile_data_migration(diff);

    MigrationPlan {
        up,
        down,
        warnings,
        breaking_changes,
        data_migrations,
    }
}

// ---------------------------------------------------------------------------
// compile_data_migration — data-level Cypher (MATCH/SET/REMOVE)
// ---------------------------------------------------------------------------

/// Generate Cypher statements for data-level migration (not DDL).
/// These handle property backfill, label renames, type coercion, and property removal.
pub fn compile_data_migration(diff: &OntologyDiff) -> Vec<DataMigrationStep> {
    let mut steps = Vec::new();

    // --- Modified nodes ---
    for node_diff in &diff.modified_nodes {
        for change in &node_diff.changes {
            match change {
                NodeChange::LabelChanged { old, new } => {
                    let old_esc = escape_identifier(old);
                    let new_esc = escape_identifier(new);
                    steps.push(DataMigrationStep {
                        description: format!(
                            "Rename node label '{old}' to '{new}': add new label and remove old label"
                        ),
                        cypher: format!(
                            "MATCH (n:{old_esc}) SET n:{new_esc} REMOVE n:{old_esc}"
                        ),
                        is_destructive: false,
                    });
                }
                NodeChange::PropertyAdded { property } => {
                    if !property.nullable {
                        if let Some(ref default) = property.default_value {
                            let label = escape_identifier(&node_diff.label);
                            let prop_name = escape_identifier(&property.name);
                            steps.push(DataMigrationStep {
                                description: format!(
                                    "Backfill required property '{}' on '{}' with default value {}",
                                    property.name, node_diff.label, default
                                ),
                                cypher: format!(
                                    "MATCH (n:{label}) WHERE n.{prop_name} IS NULL SET n.{prop_name} = {default}"
                                ),
                                is_destructive: false,
                            });
                        }
                    }
                }
                NodeChange::PropertyRemoved { property } => {
                    let label = escape_identifier(&node_diff.label);
                    let prop_name = escape_identifier(&property.name);
                    steps.push(DataMigrationStep {
                        description: format!(
                            "Remove property '{}' from all '{}' nodes",
                            property.name, node_diff.label
                        ),
                        cypher: format!("MATCH (n:{label}) REMOVE n.{prop_name}"),
                        is_destructive: true,
                    });
                }
                NodeChange::PropertyModified { property_name, changes } => {
                    for pc in changes {
                        if let PropertyChange::TypeChanged { old, new } = pc {
                            let label = escape_identifier(&node_diff.label);
                            let prop_esc = escape_identifier(property_name);
                            let (cypher, description) = match (old.as_str(), new.as_str()) {
                                ("string", "int") => (
                                    format!(
                                        "MATCH (n:{label}) SET n.{prop_esc} = toInteger(n.{prop_esc})"
                                    ),
                                    format!(
                                        "Coerce property '{}' on '{}' from string to int",
                                        property_name, node_diff.label
                                    ),
                                ),
                                ("string", "float") => (
                                    format!(
                                        "MATCH (n:{label}) SET n.{prop_esc} = toFloat(n.{prop_esc})"
                                    ),
                                    format!(
                                        "Coerce property '{}' on '{}' from string to float",
                                        property_name, node_diff.label
                                    ),
                                ),
                                ("int", "string") | ("float", "string") | ("bool", "string") => (
                                    format!(
                                        "MATCH (n:{label}) SET n.{prop_esc} = toString(n.{prop_esc})"
                                    ),
                                    format!(
                                        "Coerce property '{}' on '{}' from {} to string",
                                        property_name, node_diff.label, old
                                    ),
                                ),
                                ("int", "float") => (
                                    format!(
                                        "MATCH (n:{label}) SET n.{prop_esc} = toFloat(n.{prop_esc})"
                                    ),
                                    format!(
                                        "Coerce property '{}' on '{}' from int to float",
                                        property_name, node_diff.label
                                    ),
                                ),
                                _ => (
                                    format!(
                                        "// WARNING: No automatic coercion for {} -> {} on {}.{}",
                                        old, new, node_diff.label, property_name
                                    ),
                                    format!(
                                        "WARNING: Cannot auto-coerce property '{}' on '{}' from {} to {} — manual migration required",
                                        property_name, node_diff.label, old, new
                                    ),
                                ),
                            };
                            steps.push(DataMigrationStep {
                                description,
                                cypher,
                                is_destructive: true,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // --- Modified edges ---
    for edge_diff in &diff.modified_edges {
        for change in &edge_diff.changes {
            match change {
                EdgeChange::LabelChanged { old, new } => {
                    steps.push(DataMigrationStep {
                        description: format!(
                            "Edge type rename '{}' -> '{}': Neo4j does not support \
                             in-place relationship type changes. All existing [:{old}] \
                             relationships must be recreated as [:{new}]. This requires \
                             manual migration: create new relationships, copy properties, \
                             then delete originals.",
                            old, new
                        ),
                        cypher: format!(
                            "// Edge type rename [{old}] -> [{new}] requires manual migration:\n\
                             // MATCH (a)-[r:{old_esc}]->(b)\n\
                             // CREATE (a)-[r2:{new_esc}]->(b) SET r2 = properties(r)\n\
                             // DELETE r",
                            old_esc = escape_identifier(old),
                            new_esc = escape_identifier(new),
                        ),
                        is_destructive: true,
                    });
                }
                EdgeChange::PropertyRemoved { property } => {
                    let edge_label = escape_identifier(&edge_diff.label);
                    let prop_name = escape_identifier(&property.name);
                    steps.push(DataMigrationStep {
                        description: format!(
                            "Remove property '{}' from all [{}] relationships",
                            property.name, edge_diff.label
                        ),
                        cypher: format!(
                            "MATCH ()-[r:{edge_label}]->() REMOVE r.{prop_name}"
                        ),
                        is_destructive: true,
                    });
                }
                EdgeChange::PropertyAdded { property } => {
                    if !property.nullable {
                        if let Some(ref default) = property.default_value {
                            let edge_label = escape_identifier(&edge_diff.label);
                            let prop_name = escape_identifier(&property.name);
                            steps.push(DataMigrationStep {
                                description: format!(
                                    "Backfill required property '{}' on [{}] relationships with default value {}",
                                    property.name, edge_diff.label, default
                                ),
                                cypher: format!(
                                    "MATCH ()-[r:{edge_label}]->() WHERE r.{prop_name} IS NULL SET r.{prop_name} = {default}"
                                ),
                                is_destructive: false,
                            });
                        }
                    }
                }
                EdgeChange::PropertyModified { property_name, changes } => {
                    for pc in changes {
                        if let PropertyChange::TypeChanged { old, new } = pc {
                            let edge_label = escape_identifier(&edge_diff.label);
                            let prop_esc = escape_identifier(property_name);
                            let (cypher, description) = match (old.as_str(), new.as_str()) {
                                ("string", "int") => (
                                    format!(
                                        "MATCH ()-[r:{edge_label}]->() SET r.{prop_esc} = toInteger(r.{prop_esc})"
                                    ),
                                    format!(
                                        "Coerce property '{}' on [{}] from string to int",
                                        property_name, edge_diff.label
                                    ),
                                ),
                                ("string", "float") => (
                                    format!(
                                        "MATCH ()-[r:{edge_label}]->() SET r.{prop_esc} = toFloat(r.{prop_esc})"
                                    ),
                                    format!(
                                        "Coerce property '{}' on [{}] from string to float",
                                        property_name, edge_diff.label
                                    ),
                                ),
                                ("int", "string") | ("float", "string") | ("bool", "string") => (
                                    format!(
                                        "MATCH ()-[r:{edge_label}]->() SET r.{prop_esc} = toString(r.{prop_esc})"
                                    ),
                                    format!(
                                        "Coerce property '{}' on [{}] from {} to string",
                                        property_name, edge_diff.label, old
                                    ),
                                ),
                                ("int", "float") => (
                                    format!(
                                        "MATCH ()-[r:{edge_label}]->() SET r.{prop_esc} = toFloat(r.{prop_esc})"
                                    ),
                                    format!(
                                        "Coerce property '{}' on [{}] from int to float",
                                        property_name, edge_diff.label
                                    ),
                                ),
                                _ => (
                                    format!(
                                        "// WARNING: No automatic coercion for {} -> {} on [{}].{}",
                                        old, new, edge_diff.label, property_name
                                    ),
                                    format!(
                                        "WARNING: Cannot auto-coerce property '{}' on [{}] from {} to {} — manual migration required",
                                        property_name, edge_diff.label, old, new
                                    ),
                                ),
                            };
                            steps.push(DataMigrationStep {
                                description,
                                cypher,
                                is_destructive: true,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    steps
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::ontology_diff::compute_diff;
    use ox_core::ontology_ir::{
        Cardinality, ConstraintDef, EdgeTypeDef, IndexDef, NodeConstraint, NodeTypeDef,
        PropertyDef,
    };
    use ox_core::types::PropertyType;

    fn property(id: &str, name: &str) -> PropertyDef {
        PropertyDef {
            id: id.into(),
            name: name.to_string(),
            property_type: PropertyType::String,
            nullable: false,
            default_value: None,
            description: None,
        }
    }

    fn test_ontology() -> OntologyIR {
        OntologyIR::new(
            "ont-1".to_string(),
            "Test Ontology".to_string(),
            None,
            1,
            vec![
                NodeTypeDef {
                    id: "n1".into(),
                    label: "Person".to_string(),
                    description: Some("A person".to_string()),
                    source_table: None,
                    properties: vec![property("p1", "name"), property("p2", "age")],
                    constraints: vec![ConstraintDef {
                        id: "c1".into(),
                        constraint: NodeConstraint::Unique {
                            property_ids: vec!["p1".into()],
                        },
                    }],
                },
                NodeTypeDef {
                    id: "n2".into(),
                    label: "Company".to_string(),
                    description: None,
                    source_table: None,
                    properties: vec![property("p3", "company_name")],
                    constraints: vec![],
                },
            ],
            vec![EdgeTypeDef {
                id: "e1".into(),
                label: "WORKS_AT".to_string(),
                description: None,
                source_node_id: "n1".into(),
                target_node_id: "n2".into(),
                properties: vec![property("ep1", "since")],
                cardinality: Cardinality::ManyToOne,
            }],
            vec![IndexDef::Single {
                id: "idx1".to_string(),
                node_id: "n1".into(),
                property_id: "p1".into(),
            }],
        )
    }

    #[test]
    fn no_changes_produces_empty_plan() {
        let ont = test_ontology();
        let diff = compute_diff(&ont, &ont);
        let plan = compile_migration(&diff, &ont, &ont);

        assert!(plan.up.is_empty());
        assert!(plan.down.is_empty());
        assert!(plan.warnings.is_empty());
        assert!(plan.breaking_changes.is_empty());
        assert!(plan.data_migrations.is_empty());
    }

    #[test]
    fn added_node_creates_constraints_and_indexes() {
        let old = test_ontology();
        let mut new = old.clone();
        new.node_types.push(NodeTypeDef {
            id: "n3".into(),
            label: "Product".to_string(),
            description: None,
            source_table: None,
            properties: vec![property("p10", "sku"), property("p11", "price")],
            constraints: vec![ConstraintDef {
                id: "c3".into(),
                constraint: NodeConstraint::Unique {
                    property_ids: vec!["p10".into()],
                },
            }],
        });
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let plan = compile_migration(&diff, &old, &new);

        // Should have CREATE CONSTRAINT + CREATE INDEX statements
        assert!(!plan.up.is_empty(), "up should have DDL statements");
        assert!(
            plan.up.iter().any(|s| s.contains("Product")),
            "should reference Product label"
        );
        // Rollback should have DROP statements
        assert!(!plan.down.is_empty(), "down should have rollback DDL");
        assert!(plan.breaking_changes.is_empty());
    }

    #[test]
    fn removed_node_is_breaking_change() {
        let old = test_ontology();
        let mut new = old.clone();
        new.node_types.retain(|n| n.id != "n2");
        new.edge_types.clear();
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let plan = compile_migration(&diff, &old, &new);

        assert!(
            plan.breaking_changes
                .iter()
                .any(|s| s.contains("Company")),
            "removing Company should be breaking"
        );
    }

    #[test]
    fn label_rename_produces_warning() {
        let old = test_ontology();
        let mut new = old.clone();
        new.node_types[0].label = "Individual".to_string();
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let plan = compile_migration(&diff, &old, &new);

        assert!(
            plan.warnings
                .iter()
                .any(|w| w.contains("Person") && w.contains("Individual")),
            "label rename should produce warning"
        );
        // Should have DDL for new label constraints
        assert!(
            plan.up.iter().any(|s| s.contains("Individual")),
            "should create constraints under new label"
        );
    }

    #[test]
    fn added_required_property_is_breaking() {
        let old = test_ontology();
        let mut new = old.clone();
        new.node_types[0].properties.push(PropertyDef {
            id: "p_new".into(),
            name: "email".to_string(),
            property_type: PropertyType::String,
            nullable: false,
            default_value: None,
            description: None,
        });
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let plan = compile_migration(&diff, &old, &new);

        assert!(
            plan.breaking_changes
                .iter()
                .any(|s| s.contains("email")),
            "adding required property should be breaking"
        );
        // Should create index for non-nullable property
        assert!(
            plan.up.iter().any(|s| s.contains("email")),
            "should create index for required property"
        );
    }

    #[test]
    fn removed_property_produces_warning() {
        let old = test_ontology();
        let mut new = old.clone();
        new.node_types[0].properties.retain(|p| p.name != "age");
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let plan = compile_migration(&diff, &old, &new);

        assert!(
            plan.warnings.iter().any(|w| w.contains("age")),
            "removing property should produce warning"
        );
    }

    #[test]
    fn property_type_change_is_breaking() {
        let old = test_ontology();
        let mut new = old.clone();
        new.node_types[0].properties[1].property_type = PropertyType::Int;
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let plan = compile_migration(&diff, &old, &new);

        assert!(
            plan.breaking_changes
                .iter()
                .any(|s| s.contains("age") && s.contains("type changed")),
            "type change should be breaking"
        );
    }

    #[test]
    fn added_edge_produces_warning() {
        let old = test_ontology();
        let mut new = old.clone();
        new.edge_types.push(EdgeTypeDef {
            id: "e2".into(),
            label: "MANAGES".to_string(),
            description: None,
            source_node_id: "n1".into(),
            target_node_id: "n1".into(),
            properties: vec![],
            cardinality: Cardinality::ManyToMany,
        });
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let plan = compile_migration(&diff, &old, &new);

        assert!(
            plan.warnings.iter().any(|w| w.contains("MANAGES")),
            "added edge should produce warning"
        );
    }

    #[test]
    fn removed_edge_is_breaking() {
        let old = test_ontology();
        let mut new = old.clone();
        new.edge_types.clear();
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let plan = compile_migration(&diff, &old, &new);

        assert!(
            plan.breaking_changes
                .iter()
                .any(|s| s.contains("WORKS_AT")),
            "removing edge should be breaking"
        );
    }

    #[test]
    fn added_constraint_creates_ddl() {
        let old = test_ontology();
        let mut new = old.clone();
        new.node_types[1].constraints.push(ConstraintDef {
            id: "c_new".into(),
            constraint: NodeConstraint::Unique {
                property_ids: vec!["p3".into()],
            },
        });
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let plan = compile_migration(&diff, &old, &new);

        assert!(
            plan.up.iter().any(|s| s.contains("Company")),
            "new constraint should create DDL for Company"
        );
    }

    #[test]
    fn rollback_reverses_forward_indexes() {
        let old = test_ontology();
        let mut new = old.clone();
        new.node_types.push(NodeTypeDef {
            id: "n4".into(),
            label: "Order".to_string(),
            description: None,
            source_table: None,
            properties: vec![property("p20", "order_id")],
            constraints: vec![],
        });
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let plan = compile_migration(&diff, &old, &new);

        // Forward creates indexes, rollback drops them
        let create_count = plan.up.iter().filter(|s| s.contains("CREATE INDEX")).count();
        let drop_count = plan.down.iter().filter(|s| s.contains("DROP INDEX")).count();
        assert!(
            create_count > 0 && drop_count > 0,
            "rollback should have DROP INDEX for each CREATE INDEX"
        );
    }

    #[test]
    fn deduplicates_statements() {
        let old = test_ontology();
        let mut new = old.clone();
        // Add two properties that both generate CREATE INDEX
        new.node_types[0].properties.push(PropertyDef {
            id: "px".into(),
            name: "email".to_string(),
            property_type: PropertyType::String,
            nullable: false,
            default_value: None,
            description: None,
        });
        new.node_types[0].properties.push(PropertyDef {
            id: "py".into(),
            name: "phone".to_string(),
            property_type: PropertyType::String,
            nullable: false,
            default_value: None,
            description: None,
        });
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let plan = compile_migration(&diff, &old, &new);

        // Verify no duplicate statements
        let mut sorted = plan.up.clone();
        sorted.sort();
        let before_dedup = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), before_dedup, "should have no duplicate statements");
    }

    // -----------------------------------------------------------------------
    // Data migration tests
    // -----------------------------------------------------------------------

    #[test]
    fn data_migration_label_rename() {
        let old = test_ontology();
        let mut new = old.clone();
        new.node_types[0].label = "Individual".to_string();
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let steps = compile_data_migration(&diff);

        assert_eq!(steps.len(), 1);
        assert!(
            steps[0].cypher.contains("MATCH (n:`Person`)"),
            "should match old label: {}",
            steps[0].cypher
        );
        assert!(
            steps[0].cypher.contains("SET n:`Individual`"),
            "should set new label: {}",
            steps[0].cypher
        );
        assert!(
            steps[0].cypher.contains("REMOVE n:`Person`"),
            "should remove old label: {}",
            steps[0].cypher
        );
        assert!(!steps[0].is_destructive);
    }

    #[test]
    fn data_migration_property_added_with_default() {
        let old = test_ontology();
        let mut new = old.clone();
        new.node_types[0].properties.push(PropertyDef {
            id: "p_new".into(),
            name: "status".to_string(),
            property_type: PropertyType::String,
            nullable: false,
            default_value: Some(ox_core::types::PropertyValue::String("active".to_string())),
            description: None,
        });
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let steps = compile_data_migration(&diff);

        assert_eq!(steps.len(), 1);
        assert!(
            steps[0].cypher.contains("MATCH (n:`Person`)"),
            "should match the node label: {}",
            steps[0].cypher
        );
        assert!(
            steps[0].cypher.contains("WHERE n.`status` IS NULL"),
            "should filter for nulls: {}",
            steps[0].cypher
        );
        assert!(
            steps[0].cypher.contains("SET n.`status` ="),
            "should set the default: {}",
            steps[0].cypher
        );
        assert!(!steps[0].is_destructive);
    }

    #[test]
    fn data_migration_property_added_nullable_no_step() {
        let old = test_ontology();
        let mut new = old.clone();
        new.node_types[0].properties.push(PropertyDef {
            id: "p_new".into(),
            name: "nickname".to_string(),
            property_type: PropertyType::String,
            nullable: true,
            default_value: None,
            description: None,
        });
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let steps = compile_data_migration(&diff);

        // Nullable property with no default: no data migration needed
        assert!(steps.is_empty(), "nullable property should not generate data migration");
    }

    #[test]
    fn data_migration_property_added_required_no_default_no_step() {
        let old = test_ontology();
        let mut new = old.clone();
        new.node_types[0].properties.push(PropertyDef {
            id: "p_new".into(),
            name: "email".to_string(),
            property_type: PropertyType::String,
            nullable: false,
            default_value: None,
            description: None,
        });
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let steps = compile_data_migration(&diff);

        // Required but no default: DDL-level breaking change, no data migration generated
        assert!(steps.is_empty(), "required property without default should not generate data migration");
    }

    #[test]
    fn data_migration_property_removed() {
        let old = test_ontology();
        let mut new = old.clone();
        new.node_types[0].properties.retain(|p| p.name != "age");
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let steps = compile_data_migration(&diff);

        assert_eq!(steps.len(), 1);
        assert!(
            steps[0].cypher.contains("MATCH (n:`Person`)"),
            "should match the node label: {}",
            steps[0].cypher
        );
        assert!(
            steps[0].cypher.contains("REMOVE n.`age`"),
            "should remove the property: {}",
            steps[0].cypher
        );
        assert!(steps[0].is_destructive);
    }

    #[test]
    fn data_migration_property_type_string_to_int() {
        let old = test_ontology();
        let mut new = old.clone();
        // age is String in test_ontology, change to Int
        new.node_types[0].properties[1].property_type = PropertyType::Int;
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let steps = compile_data_migration(&diff);

        assert_eq!(steps.len(), 1);
        assert!(
            steps[0].cypher.contains("toInteger(n.`age`)"),
            "should use toInteger for string->int coercion: {}",
            steps[0].cypher
        );
        assert!(steps[0].is_destructive);
    }

    #[test]
    fn data_migration_property_type_string_to_float() {
        let old = test_ontology();
        let mut new = old.clone();
        new.node_types[0].properties[1].property_type = PropertyType::Float;
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let steps = compile_data_migration(&diff);

        assert_eq!(steps.len(), 1);
        assert!(
            steps[0].cypher.contains("toFloat(n.`age`)"),
            "should use toFloat for string->float coercion: {}",
            steps[0].cypher
        );
        assert!(steps[0].is_destructive);
    }

    #[test]
    fn data_migration_property_type_int_to_string() {
        let old = test_ontology();
        // Create an "old" where age is Int
        let old_modified = {
            let mut m = old.clone();
            m.node_types[0].properties[1].property_type = PropertyType::Int;
            m.rebuild_indices();
            m
        };
        let diff = compute_diff(&old_modified, &old);
        let steps = compile_data_migration(&diff);

        assert_eq!(steps.len(), 1);
        assert!(
            steps[0].cypher.contains("toString(n.`age`)"),
            "should use toString for int->string coercion: {}",
            steps[0].cypher
        );
        assert!(steps[0].is_destructive);
    }

    #[test]
    fn data_migration_property_type_unsupported_warns() {
        let old = test_ontology();
        let mut new = old.clone();
        new.node_types[0].properties[1].property_type = PropertyType::Date;
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let steps = compile_data_migration(&diff);

        assert_eq!(steps.len(), 1);
        assert!(
            steps[0].cypher.starts_with("// WARNING"),
            "unsupported coercion should produce a comment: {}",
            steps[0].cypher
        );
        assert!(
            steps[0].description.contains("manual migration"),
            "description should mention manual migration"
        );
        assert!(steps[0].is_destructive);
    }

    #[test]
    fn data_migration_edge_label_rename_requires_manual() {
        let old = test_ontology();
        let mut new = old.clone();
        new.edge_types[0].label = "EMPLOYED_AT".to_string();
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let steps = compile_data_migration(&diff);

        assert_eq!(steps.len(), 1);
        assert!(
            steps[0].cypher.contains("//"),
            "edge rename should be a comment (manual migration)"
        );
        assert!(
            steps[0].description.contains("manual migration"),
            "should explain manual migration is required"
        );
        assert!(steps[0].is_destructive);
    }

    #[test]
    fn data_migration_edge_property_removed() {
        let old = test_ontology();
        let mut new = old.clone();
        new.edge_types[0].properties.clear();
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let steps = compile_data_migration(&diff);

        assert_eq!(steps.len(), 1);
        assert!(
            steps[0].cypher.contains("MATCH ()-[r:`WORKS_AT`]->()"),
            "should match edge type: {}",
            steps[0].cypher
        );
        assert!(
            steps[0].cypher.contains("REMOVE r.`since`"),
            "should remove the property: {}",
            steps[0].cypher
        );
        assert!(steps[0].is_destructive);
    }

    #[test]
    fn data_migration_integrated_in_plan() {
        let old = test_ontology();
        let mut new = old.clone();
        // Remove a property to trigger a data migration
        new.node_types[0].properties.retain(|p| p.name != "age");
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);
        let plan = compile_migration(&diff, &old, &new);

        assert!(
            !plan.data_migrations.is_empty(),
            "MigrationPlan should include data_migrations"
        );
        assert!(
            plan.data_migrations.iter().any(|s| s.cypher.contains("REMOVE n.`age`")),
            "data_migrations should contain the property removal"
        );
    }
}

/// Attempt to reverse a CREATE INDEX statement into a DROP statement.
/// Constraint rollbacks are not auto-generated because Neo4j 5.x requires
/// `DROP CONSTRAINT <name>` and the auto-generated constraint name is unknown
/// at compile time. Rollback for constraints must be done via
/// `SHOW CONSTRAINTS YIELD name WHERE ...` at runtime.
fn reverse_index_stmt(create_stmt: &str) -> Option<String> {
    if create_stmt.starts_with("CREATE INDEX") {
        Some(create_stmt.replace("CREATE INDEX IF NOT EXISTS", "DROP INDEX IF EXISTS"))
    } else {
        None
    }
}
