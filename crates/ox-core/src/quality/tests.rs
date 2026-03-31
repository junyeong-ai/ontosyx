use super::*;
use crate::ontology_ir::{Cardinality, EdgeTypeDef, NodeTypeDef, OntologyIR, PropertyDef};
use crate::source_mapping::SourceMapping;
use crate::source_schema::{
    SourceColumnDef, ForeignKeyDef, SourceProfile, SourceSchema, SourceTableDef, TableProfile,
};
use crate::types::PropertyType;

fn property(name: &str) -> PropertyDef {
    property_typed(name, PropertyType::String)
}

fn property_typed(name: &str, property_type: PropertyType) -> PropertyDef {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    PropertyDef {
        id: format!("prop-{name}-{seq}").into(),
        name: name.to_string(),
        property_type,
        nullable: false,
        default_value: None,
        description: Some("desc".to_string()),
    }
}

fn mapping_with_tables(entries: &[(&str, &str)]) -> SourceMapping {
    let mut m = SourceMapping::new();
    for (node_id, table) in entries {
        m.node_tables.insert(node_id.to_string(), table.to_string());
    }
    m
}

#[test]
fn flags_unmapped_tables_and_missing_edges() {
    let ontology = OntologyIR::new(
        "onto".to_string(),
        "Test".to_string(),
        None,
        1,
        vec![NodeTypeDef {
            id: "node-user".into(),
            label: "User".to_string(),
            description: Some("users".to_string()),
            source_table: None,
            properties: vec![property("id")],
            constraints: vec![],
        }],
        vec![],
        vec![],
    );
    let mapping = mapping_with_tables(&[("node-user", "users")]);
    let schema = SourceSchema {
        source_type: "postgresql".to_string(),
        tables: vec![
            SourceTableDef {
                name: "users".to_string(),
                columns: vec![SourceColumnDef {
                    name: "id".to_string(),
                    data_type: "int".to_string(),
                    nullable: false,
                }],
                primary_key: vec!["id".to_string()],
            },
            SourceTableDef {
                name: "orders".to_string(),
                columns: vec![SourceColumnDef {
                    name: "id".to_string(),
                    data_type: "int".to_string(),
                    nullable: false,
                }],
                primary_key: vec!["id".to_string()],
            },
        ],
        foreign_keys: vec![ForeignKeyDef {
            from_table: "orders".to_string(),
            from_column: "user_id".to_string(),
            to_table: "users".to_string(),
            to_column: "id".to_string(),
            inferred: false,
        }],
    };
    let profile = SourceProfile {
        table_profiles: vec![
            TableProfile {
                table_name: "users".to_string(),
                row_count: 10,
                column_stats: vec![],
            },
            TableProfile {
                table_name: "orders".to_string(),
                row_count: 10,
                column_stats: vec![],
            },
        ],
    };

    let report = assess_quality(&ontology, Some(&schema), Some(&profile), &mapping, &[], &[]);

    assert!(report.gaps.iter().any(|gap| {
        matches!(gap.category, QualityGapCategory::UnmappedSourceTable)
            && matches!(&gap.location, QualityGapRef::SourceTable { table } if table == "orders")
    }));
    // FK from orders->users is NOT reported as MissingForeignKeyEdge because
    // the from_table "orders" is unmapped (already reported as UnmappedSourceTable).
    assert!(
        !report
            .gaps
            .iter()
            .any(|gap| matches!(gap.category, QualityGapCategory::MissingForeignKeyEdge))
    );
}

#[test]
fn flags_missing_fk_edge_when_both_tables_mapped() {
    let ontology = OntologyIR::new(
        "onto".to_string(),
        "Test".to_string(),
        None,
        1,
        vec![
            NodeTypeDef {
                id: "node-user".into(),
                label: "User".to_string(),
                description: Some("desc".to_string()),
                source_table: None,
                properties: vec![property("id")],
                constraints: vec![],
            },
            NodeTypeDef {
                id: "node-order".into(),
                label: "Order".to_string(),
                description: Some("desc".to_string()),
                source_table: None,
                properties: vec![property("id")],
                constraints: vec![],
            },
        ],
        vec![], // No edge for the FK
        vec![],
    );
    let mapping = mapping_with_tables(&[("node-user", "users"), ("node-order", "orders")]);
    let schema = SourceSchema {
        source_type: "postgresql".to_string(),
        tables: vec![
            SourceTableDef {
                name: "users".to_string(),
                columns: vec![SourceColumnDef {
                    name: "id".to_string(),
                    data_type: "int".to_string(),
                    nullable: false,
                }],
                primary_key: vec!["id".to_string()],
            },
            SourceTableDef {
                name: "orders".to_string(),
                columns: vec![SourceColumnDef {
                    name: "id".to_string(),
                    data_type: "int".to_string(),
                    nullable: false,
                }],
                primary_key: vec!["id".to_string()],
            },
        ],
        foreign_keys: vec![ForeignKeyDef {
            from_table: "orders".to_string(),
            from_column: "user_id".to_string(),
            to_table: "users".to_string(),
            to_column: "id".to_string(),
            inferred: false,
        }],
    };
    let profile = SourceProfile {
        table_profiles: vec![],
    };

    let report = assess_quality(&ontology, Some(&schema), Some(&profile), &mapping, &[], &[]);

    assert!(
        report
            .gaps
            .iter()
            .any(|gap| matches!(gap.category, QualityGapCategory::MissingForeignKeyEdge))
    );
}

#[test]
fn skips_excluded_tables_from_coverage_checks() {
    let ontology = OntologyIR::new(
        "onto".to_string(),
        "Test".to_string(),
        None,
        1,
        vec![NodeTypeDef {
            id: "node-user".into(),
            label: "User".to_string(),
            description: Some("users".to_string()),
            source_table: None,
            properties: vec![property("id")],
            constraints: vec![],
        }],
        vec![EdgeTypeDef {
            id: "edge-belongs-to".into(),
            label: "BELONGS_TO".to_string(),
            description: Some("edge".to_string()),
            source_node_id: "node-order".into(),
            target_node_id: "node-user".into(),
            properties: vec![],
            cardinality: Cardinality::ManyToOne,
        }],
        vec![],
    );
    let mapping = mapping_with_tables(&[("node-user", "users")]);
    let schema = SourceSchema {
        source_type: "postgresql".to_string(),
        tables: vec![SourceTableDef {
            name: "orders".to_string(),
            columns: vec![SourceColumnDef {
                name: "id".to_string(),
                data_type: "int".to_string(),
                nullable: false,
            }],
            primary_key: vec!["id".to_string()],
        }],
        foreign_keys: vec![],
    };
    let profile = SourceProfile {
        table_profiles: vec![TableProfile {
            table_name: "orders".to_string(),
            row_count: 10,
            column_stats: vec![],
        }],
    };

    let report = assess_quality(
        &ontology,
        Some(&schema),
        Some(&profile),
        &mapping,
        &["orders".to_string()],
        &[],
    );

    assert!(!report.gaps.iter().any(|gap| {
        matches!(gap.category, QualityGapCategory::UnmappedSourceTable)
            || matches!(gap.category, QualityGapCategory::MissingForeignKeyEdge)
    }));
}

#[test]
fn column_clarifications_suppress_data_observation_gaps() {
    use crate::source_analysis::ColumnClarification;
    use crate::source_schema::ColumnStats;

    let ontology = OntologyIR::new(
        "onto".to_string(),
        "Test".to_string(),
        None,
        1,
        vec![NodeTypeDef {
            id: "node-store".into(),
            label: "Store".to_string(),
            description: Some("stores".to_string()),
            source_table: None,
            properties: vec![property("id"), property("type_code"), property("status")],
            constraints: vec![],
        }],
        vec![],
        vec![],
    );
    let mapping = mapping_with_tables(&[("node-store", "stores")]);
    let schema = SourceSchema {
        source_type: "postgresql".to_string(),
        tables: vec![SourceTableDef {
            name: "stores".to_string(),
            columns: vec![
                SourceColumnDef { name: "id".to_string(), data_type: "int".to_string(), nullable: false },
                SourceColumnDef { name: "type_code".to_string(), data_type: "int".to_string(), nullable: false },
                SourceColumnDef { name: "status".to_string(), data_type: "varchar".to_string(), nullable: true },
            ],
            primary_key: vec!["id".to_string()],
        }],
        foreign_keys: vec![],
    };
    let profile = SourceProfile {
        table_profiles: vec![TableProfile {
            table_name: "stores".to_string(),
            row_count: 20,
            column_stats: vec![
                ColumnStats {
                    column_name: "type_code".to_string(),
                    null_count: 0,
                    distinct_count: 3,
                    sample_values: vec!["1".to_string(), "2".to_string(), "3".to_string()],
                    min_value: Some("1".to_string()),
                    max_value: Some("3".to_string()),
                },
                ColumnStats {
                    column_name: "status".to_string(),
                    null_count: 0,
                    distinct_count: 1,
                    sample_values: vec!["active".to_string()],
                    min_value: Some("active".to_string()),
                    max_value: Some("active".to_string()),
                },
            ],
        }],
    };

    // Without clarifications: should have NumericEnumCode + SingleValueBias gaps
    let report_no_clarify = assess_quality(&ontology, Some(&schema), Some(&profile), &mapping, &[], &[]);
    assert!(report_no_clarify.gaps.iter().any(|g| matches!(g.category, QualityGapCategory::NumericEnumCode)));
    assert!(report_no_clarify.gaps.iter().any(|g| matches!(g.category, QualityGapCategory::SingleValueBias)));

    // With clarifications: those gaps should be suppressed
    let clarifications = vec![
        ColumnClarification {
            table: "stores".to_string(),
            column: "type_code".to_string(),
            hint: "1=normal, 2=flagship, 3=outlet".to_string(),
        },
        ColumnClarification {
            table: "stores".to_string(),
            column: "status".to_string(),
            hint: "active is the only status for now".to_string(),
        },
    ];
    let report_clarified = assess_quality(&ontology, Some(&schema), Some(&profile), &mapping, &[], &clarifications);
    assert!(!report_clarified.gaps.iter().any(|g| matches!(g.category, QualityGapCategory::NumericEnumCode)));
    assert!(!report_clarified.gaps.iter().any(|g| matches!(g.category, QualityGapCategory::SingleValueBias)));
}

#[test]
fn junction_table_not_flagged_as_unmapped() {
    // A pure junction table (order_items) linking orders and products
    // should not be flagged as unmapped when an edge exists between those nodes.
    let ontology = OntologyIR::new(
        "onto".to_string(),
        "Test".to_string(),
        None,
        1,
        vec![
            NodeTypeDef {
                id: "node-order".into(),
                label: "Order".to_string(),
                description: Some("desc".to_string()),
                source_table: None,
                properties: vec![property("id")],
                constraints: vec![],
            },
            NodeTypeDef {
                id: "node-product".into(),
                label: "Product".to_string(),
                description: Some("desc".to_string()),
                source_table: None,
                properties: vec![property("id")],
                constraints: vec![],
            },
        ],
        vec![EdgeTypeDef {
            id: "edge-contains".into(),
            label: "CONTAINS".to_string(),
            description: Some("order contains product".to_string()),
            source_node_id: "node-order".into(),
            target_node_id: "node-product".into(),
            properties: vec![],
            cardinality: Cardinality::ManyToMany,
        }],
        vec![],
    );
    let mapping = mapping_with_tables(&[("node-order", "orders"), ("node-product", "products")]);
    let schema = SourceSchema {
        source_type: "postgresql".to_string(),
        tables: vec![
            SourceTableDef {
                name: "orders".to_string(),
                columns: vec![SourceColumnDef { name: "id".to_string(), data_type: "int".to_string(), nullable: false }],
                primary_key: vec!["id".to_string()],
            },
            SourceTableDef {
                name: "products".to_string(),
                columns: vec![SourceColumnDef { name: "id".to_string(), data_type: "int".to_string(), nullable: false }],
                primary_key: vec!["id".to_string()],
            },
            SourceTableDef {
                name: "order_items".to_string(),
                columns: vec![
                    SourceColumnDef { name: "id".to_string(), data_type: "int".to_string(), nullable: false },
                    SourceColumnDef { name: "order_id".to_string(), data_type: "int".to_string(), nullable: false },
                    SourceColumnDef { name: "product_id".to_string(), data_type: "int".to_string(), nullable: false },
                ],
                primary_key: vec!["id".to_string()],
            },
        ],
        foreign_keys: vec![
            ForeignKeyDef {
                from_table: "order_items".to_string(),
                from_column: "order_id".to_string(),
                to_table: "orders".to_string(),
                to_column: "id".to_string(),
                inferred: false,
            },
            ForeignKeyDef {
                from_table: "order_items".to_string(),
                from_column: "product_id".to_string(),
                to_table: "products".to_string(),
                to_column: "id".to_string(),
                inferred: false,
            },
        ],
    };
    let profile = SourceProfile { table_profiles: vec![] };

    let report = assess_quality(&ontology, Some(&schema), Some(&profile), &mapping, &[], &[]);

    // order_items is a pure junction table with an edge between orders and products,
    // so it should NOT be flagged as unmapped.
    assert!(
        !report.gaps.iter().any(|gap| {
            matches!(gap.category, QualityGapCategory::UnmappedSourceTable)
                && matches!(&gap.location, QualityGapRef::SourceTable { table } if table == "order_items")
        }),
        "Junction table 'order_items' should not be flagged as unmapped when represented by an edge"
    );
}

/// Assess ontology quality without source data (ontology-only checks).
fn assess_ontology_only(ontology: &OntologyIR) -> OntologyQualityReport {
    assess_quality(ontology, None, None, &SourceMapping::new(), &[], &[])
}

// ---------------------------------------------------------------------------
// OrphanNode tests
// ---------------------------------------------------------------------------

#[test]
fn flags_orphan_node_with_no_edges() {
    let ontology = OntologyIR::new(
        "onto".to_string(),
        "Test".to_string(),
        None,
        1,
        vec![
            NodeTypeDef {
                id: "node-user".into(),
                label: "User".to_string(),
                description: Some("desc".to_string()),
                source_table: None,
                properties: vec![property("id")],
                constraints: vec![],
            },
            NodeTypeDef {
                id: "node-product".into(),
                label: "Product".to_string(),
                description: Some("desc".to_string()),
                source_table: None,
                properties: vec![property("id")],
                constraints: vec![],
            },
        ],
        // Only one edge connecting User, Product is orphaned
        vec![EdgeTypeDef {
            id: "edge-self".into(),
            label: "KNOWS".to_string(),
            description: Some("desc".to_string()),
            source_node_id: "node-user".into(),
            target_node_id: "node-user".into(),
            properties: vec![],
            cardinality: Cardinality::ManyToMany,
        }],
        vec![],
    );

    let report = assess_ontology_only(&ontology);
    assert!(
        report.gaps.iter().any(|g| {
            matches!(g.category, QualityGapCategory::OrphanNode)
                && matches!(&g.location, QualityGapRef::Node { label, .. } if label == "Product")
        }),
        "Product should be flagged as orphan"
    );
}

#[test]
fn no_orphan_when_all_nodes_connected() {
    let ontology = OntologyIR::new(
        "onto".to_string(),
        "Test".to_string(),
        None,
        1,
        vec![
            NodeTypeDef {
                id: "node-user".into(),
                label: "User".to_string(),
                description: Some("desc".to_string()),
                source_table: None,
                properties: vec![property("id")],
                constraints: vec![],
            },
            NodeTypeDef {
                id: "node-order".into(),
                label: "Order".to_string(),
                description: Some("desc".to_string()),
                source_table: None,
                properties: vec![property("id")],
                constraints: vec![],
            },
        ],
        vec![EdgeTypeDef {
            id: "edge-placed".into(),
            label: "PLACED".to_string(),
            description: Some("desc".to_string()),
            source_node_id: "node-user".into(),
            target_node_id: "node-order".into(),
            properties: vec![],
            cardinality: Cardinality::OneToMany,
        }],
        vec![],
    );

    let report = assess_ontology_only(&ontology);
    assert!(
        !report.gaps.iter().any(|g| matches!(g.category, QualityGapCategory::OrphanNode)),
        "No orphan nodes expected"
    );
}

#[test]
fn no_orphan_when_single_node() {
    let ontology = OntologyIR::new(
        "onto".to_string(),
        "Test".to_string(),
        None,
        1,
        vec![NodeTypeDef {
            id: "node-user".into(),
            label: "User".to_string(),
            description: Some("desc".to_string()),
            source_table: None,
            properties: vec![property("id")],
            constraints: vec![],
        }],
        vec![],
        vec![],
    );

    let report = assess_ontology_only(&ontology);
    assert!(
        !report.gaps.iter().any(|g| matches!(g.category, QualityGapCategory::OrphanNode)),
        "Single-node ontology should not flag orphan"
    );
}

// ---------------------------------------------------------------------------
// PropertyTypeInconsistency tests
// ---------------------------------------------------------------------------

#[test]
fn flags_property_type_inconsistency() {
    let ontology = OntologyIR::new(
        "onto".to_string(),
        "Test".to_string(),
        None,
        1,
        vec![
            NodeTypeDef {
                id: "node-customer".into(),
                label: "Customer".to_string(),
                description: Some("desc".to_string()),
                source_table: None,
                properties: vec![
                    property("id"),
                    property_typed("email", PropertyType::String),
                ],
                constraints: vec![],
            },
            NodeTypeDef {
                id: "node-supplier".into(),
                label: "Supplier".to_string(),
                description: Some("desc".to_string()),
                source_table: None,
                properties: vec![
                    property("id"),
                    property_typed("email", PropertyType::Int), // inconsistent type
                ],
                constraints: vec![],
            },
        ],
        vec![EdgeTypeDef {
            id: "edge-supplies".into(),
            label: "SUPPLIES".to_string(),
            description: Some("desc".to_string()),
            source_node_id: "node-supplier".into(),
            target_node_id: "node-customer".into(),
            properties: vec![],
            cardinality: Cardinality::ManyToMany,
        }],
        vec![],
    );

    let report = assess_ontology_only(&ontology);
    assert!(
        report.gaps.iter().any(|g| matches!(g.category, QualityGapCategory::PropertyTypeInconsistency)),
        "Should flag email with different types"
    );
}

#[test]
fn no_property_type_inconsistency_when_same_type() {
    let ontology = OntologyIR::new(
        "onto".to_string(),
        "Test".to_string(),
        None,
        1,
        vec![
            NodeTypeDef {
                id: "node-customer".into(),
                label: "Customer".to_string(),
                description: Some("desc".to_string()),
                source_table: None,
                properties: vec![
                    property("id"),
                    property_typed("email", PropertyType::String),
                ],
                constraints: vec![],
            },
            NodeTypeDef {
                id: "node-supplier".into(),
                label: "Supplier".to_string(),
                description: Some("desc".to_string()),
                source_table: None,
                properties: vec![
                    property("id"),
                    property_typed("email", PropertyType::String), // same type
                ],
                constraints: vec![],
            },
        ],
        vec![EdgeTypeDef {
            id: "edge-supplies".into(),
            label: "SUPPLIES".to_string(),
            description: Some("desc".to_string()),
            source_node_id: "node-supplier".into(),
            target_node_id: "node-customer".into(),
            properties: vec![],
            cardinality: Cardinality::ManyToMany,
        }],
        vec![],
    );

    let report = assess_ontology_only(&ontology);
    assert!(
        !report.gaps.iter().any(|g| matches!(g.category, QualityGapCategory::PropertyTypeInconsistency)),
        "Same property types should not be flagged"
    );
}

// ---------------------------------------------------------------------------
// HubNode tests
// ---------------------------------------------------------------------------

#[test]
fn flags_hub_node_with_many_edges() {
    // Create a node connected to 9 edges (> threshold of 8)
    let mut nodes = vec![NodeTypeDef {
        id: "node-center".into(),
        label: "Center".to_string(),
        description: Some("desc".to_string()),
        source_table: None,
        properties: vec![property("id")],
        constraints: vec![],
    }];
    let mut edges = Vec::new();

    for i in 0..9 {
        let node_id = format!("node-sat-{i}");
        nodes.push(NodeTypeDef {
            id: node_id.clone().into(),
            label: format!("Satellite{i}"),
            description: Some("desc".to_string()),
            source_table: None,
            properties: vec![property("id")],
            constraints: vec![],
        });
        edges.push(EdgeTypeDef {
            id: format!("edge-{i}").into(),
            label: format!("REL_{i}"),
            description: Some("desc".to_string()),
            source_node_id: "node-center".into(),
            target_node_id: node_id.into(),
            properties: vec![],
            cardinality: Cardinality::ManyToMany,
        });
    }

    let ontology = OntologyIR::new(
        "onto".to_string(),
        "Test".to_string(),
        None,
        1,
        nodes,
        edges,
        vec![],
    );

    let report = assess_ontology_only(&ontology);
    assert!(
        report.gaps.iter().any(|g| {
            matches!(g.category, QualityGapCategory::HubNode)
                && matches!(&g.location, QualityGapRef::Node { label, .. } if label == "Center")
        }),
        "Center with 9 edges should be flagged as hub"
    );
}

#[test]
fn no_hub_node_under_threshold() {
    let ontology = OntologyIR::new(
        "onto".to_string(),
        "Test".to_string(),
        None,
        1,
        vec![
            NodeTypeDef {
                id: "node-a".into(),
                label: "NodeA".to_string(),
                description: Some("desc".to_string()),
                source_table: None,
                properties: vec![property("id")],
                constraints: vec![],
            },
            NodeTypeDef {
                id: "node-b".into(),
                label: "NodeB".to_string(),
                description: Some("desc".to_string()),
                source_table: None,
                properties: vec![property("id")],
                constraints: vec![],
            },
        ],
        vec![EdgeTypeDef {
            id: "edge-1".into(),
            label: "REL".to_string(),
            description: Some("desc".to_string()),
            source_node_id: "node-a".into(),
            target_node_id: "node-b".into(),
            properties: vec![],
            cardinality: Cardinality::ManyToMany,
        }],
        vec![],
    );

    let report = assess_ontology_only(&ontology);
    assert!(
        !report.gaps.iter().any(|g| matches!(g.category, QualityGapCategory::HubNode)),
        "2-node graph should not flag hub"
    );
}

// ---------------------------------------------------------------------------
// OverloadedProperty tests
// ---------------------------------------------------------------------------

#[test]
fn flags_overloaded_property_on_many_nodes() {
    // Create 4 nodes all with a "status" property (> threshold of 3)
    let nodes: Vec<_> = (0..4)
        .map(|i| NodeTypeDef {
            id: format!("node-{i}").into(),
            label: format!("Type{i}"),
            description: Some("desc".to_string()),
            source_table: None,
            properties: vec![property("id"), property("status")],
            constraints: vec![],
        })
        .collect();

    // Connect them in a chain so they're not orphans
    let edges: Vec<_> = (0..3)
        .map(|i| EdgeTypeDef {
            id: format!("edge-{i}").into(),
            label: format!("REL_{i}"),
            description: Some("desc".to_string()),
            source_node_id: format!("node-{i}").into(),
            target_node_id: format!("node-{}", i + 1).into(),
            properties: vec![],
            cardinality: Cardinality::ManyToMany,
        })
        .collect();

    let ontology = OntologyIR::new(
        "onto".to_string(),
        "Test".to_string(),
        None,
        1,
        nodes,
        edges,
        vec![],
    );

    let report = assess_ontology_only(&ontology);
    assert!(
        report.gaps.iter().any(|g| matches!(g.category, QualityGapCategory::OverloadedProperty)),
        "status on 4 nodes should be flagged as overloaded"
    );
}

#[test]
fn no_overloaded_property_under_threshold() {
    // 3 nodes with "name" — exactly at threshold, should NOT flag
    let nodes: Vec<_> = (0..3)
        .map(|i| NodeTypeDef {
            id: format!("node-{i}").into(),
            label: format!("Type{i}"),
            description: Some("desc".to_string()),
            source_table: None,
            properties: vec![property("id"), property("name")],
            constraints: vec![],
        })
        .collect();

    let edges: Vec<_> = (0..2)
        .map(|i| EdgeTypeDef {
            id: format!("edge-{i}").into(),
            label: format!("REL_{i}"),
            description: Some("desc".to_string()),
            source_node_id: format!("node-{i}").into(),
            target_node_id: format!("node-{}", i + 1).into(),
            properties: vec![],
            cardinality: Cardinality::ManyToMany,
        })
        .collect();

    let ontology = OntologyIR::new(
        "onto".to_string(),
        "Test".to_string(),
        None,
        1,
        nodes,
        edges,
        vec![],
    );

    let report = assess_ontology_only(&ontology);
    assert!(
        !report.gaps.iter().any(|g| matches!(g.category, QualityGapCategory::OverloadedProperty)),
        "3 nodes at threshold should not flag"
    );
}

// ---------------------------------------------------------------------------
// SelfReferentialEdge tests
// ---------------------------------------------------------------------------

#[test]
fn flags_self_referential_edge() {
    let ontology = OntologyIR::new(
        "onto".to_string(),
        "Test".to_string(),
        None,
        1,
        vec![NodeTypeDef {
            id: "node-employee".into(),
            label: "Employee".to_string(),
            description: Some("desc".to_string()),
            source_table: None,
            properties: vec![property("id")],
            constraints: vec![],
        }],
        vec![EdgeTypeDef {
            id: "edge-manages".into(),
            label: "MANAGES".to_string(),
            description: Some("desc".to_string()),
            source_node_id: "node-employee".into(),
            target_node_id: "node-employee".into(),
            properties: vec![],
            cardinality: Cardinality::OneToMany,
        }],
        vec![],
    );

    let report = assess_ontology_only(&ontology);
    assert!(
        report.gaps.iter().any(|g| {
            matches!(g.category, QualityGapCategory::SelfReferentialEdge)
                && matches!(&g.location, QualityGapRef::Edge { label, .. } if label == "MANAGES")
        }),
        "Self-referential edge should be flagged"
    );
}

#[test]
fn no_self_referential_for_normal_edges() {
    let ontology = OntologyIR::new(
        "onto".to_string(),
        "Test".to_string(),
        None,
        1,
        vec![
            NodeTypeDef {
                id: "node-a".into(),
                label: "NodeA".to_string(),
                description: Some("desc".to_string()),
                source_table: None,
                properties: vec![property("id")],
                constraints: vec![],
            },
            NodeTypeDef {
                id: "node-b".into(),
                label: "NodeB".to_string(),
                description: Some("desc".to_string()),
                source_table: None,
                properties: vec![property("id")],
                constraints: vec![],
            },
        ],
        vec![EdgeTypeDef {
            id: "edge-rel".into(),
            label: "RELATES".to_string(),
            description: Some("desc".to_string()),
            source_node_id: "node-a".into(),
            target_node_id: "node-b".into(),
            properties: vec![],
            cardinality: Cardinality::ManyToMany,
        }],
        vec![],
    );

    let report = assess_ontology_only(&ontology);
    assert!(
        !report.gaps.iter().any(|g| matches!(g.category, QualityGapCategory::SelfReferentialEdge)),
        "Normal edges should not be flagged as self-referential"
    );
}

// ---------------------------------------------------------------------------
// Well-designed ontology should have no false positives from new rules
// ---------------------------------------------------------------------------

#[test]
fn well_designed_ontology_has_no_structural_gaps() {
    let ontology = OntologyIR::new(
        "onto".to_string(),
        "E-commerce".to_string(),
        Some("Well-designed e-commerce ontology".to_string()),
        1,
        vec![
            NodeTypeDef {
                id: "node-customer".into(),
                label: "Customer".to_string(),
                description: Some("A customer who places orders".to_string()),
                source_table: None,
                properties: vec![
                    property("id"),
                    property("name"),
                    property("email"),
                ],
                constraints: vec![],
            },
            NodeTypeDef {
                id: "node-order".into(),
                label: "Order".to_string(),
                description: Some("An order placed by a customer".to_string()),
                source_table: None,
                properties: vec![
                    property("id"),
                    property_typed("total", PropertyType::Float),
                    property("created_at"),
                ],
                constraints: vec![],
            },
            NodeTypeDef {
                id: "node-product".into(),
                label: "Product".to_string(),
                description: Some("A product available for purchase".to_string()),
                source_table: None,
                properties: vec![
                    property("id"),
                    property("name"),
                    property_typed("price", PropertyType::Float),
                ],
                constraints: vec![],
            },
        ],
        vec![
            EdgeTypeDef {
                id: "edge-placed".into(),
                label: "PLACED".to_string(),
                description: Some("Customer placed an order".to_string()),
                source_node_id: "node-customer".into(),
                target_node_id: "node-order".into(),
                properties: vec![],
                cardinality: Cardinality::OneToMany,
            },
            EdgeTypeDef {
                id: "edge-contains".into(),
                label: "CONTAINS".to_string(),
                description: Some("Order contains a product".to_string()),
                source_node_id: "node-order".into(),
                target_node_id: "node-product".into(),
                properties: vec![
                    property_typed("quantity", PropertyType::Int),
                ],
                cardinality: Cardinality::ManyToMany,
            },
        ],
        vec![],
    );

    let report = assess_ontology_only(&ontology);

    // A well-designed ontology should have no structural gaps
    let structural_gaps: Vec<_> = report.gaps.iter().filter(|g| {
        matches!(
            g.category,
            QualityGapCategory::OrphanNode
                | QualityGapCategory::PropertyTypeInconsistency
                | QualityGapCategory::HubNode
                | QualityGapCategory::OverloadedProperty
                | QualityGapCategory::SelfReferentialEdge
        )
    }).collect();

    assert!(
        structural_gaps.is_empty(),
        "Well-designed ontology should produce no structural gaps, got: {:?}",
        structural_gaps.iter().map(|g| format!("{:?}: {}", g.category, g.issue)).collect::<Vec<_>>()
    );
}
