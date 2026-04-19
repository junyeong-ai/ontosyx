//! Standard evaluation fixtures: ontology and eval cases.
//!
//! The e-commerce ontology and its 20 evaluation cases provide a comprehensive
//! baseline for measuring query translation accuracy across all query categories.

use crate::graph_label::GraphLabel;
use crate::i18n::LocalizedText;
use crate::ontology_ir::{
    Cardinality, ConstraintDef, EdgeTypeDef, IndexDef, NodeConstraint, NodeTypeDef, OntologyIR,
    PropertyDef,
};
use crate::property_key::PropertyKey;
use crate::types::PropertyType;

use super::cases::{EvalCase, EvalCategory, ExpectedOp};

/// Build a `GraphLabel` from an inline literal — every call in this
/// fixture is authored by hand, so a literal that fails validation is
/// a test-code bug caught by any test exercising the ontology.
#[allow(clippy::expect_used)]
fn gl(s: &'static str) -> GraphLabel {
    GraphLabel::new(s).expect("fixture label literal must satisfy GraphLabel invariants")
}

#[allow(clippy::expect_used)]
fn pk(s: &str) -> PropertyKey {
    PropertyKey::new(s).expect("fixture property name must satisfy PropertyKey invariants")
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn prop(id: &str, name: &str, ty: PropertyType, desc: Option<&str>) -> PropertyDef {
    PropertyDef {
        id: id.into(),
        name: pk(name),
        property_type: ty,
        nullable: false,
        default_value: None,
        description: LocalizedText::from(desc),
        classification: None,
        ..Default::default()
    }
}

fn nullable_prop(id: &str, name: &str, ty: PropertyType, desc: Option<&str>) -> PropertyDef {
    PropertyDef {
        id: id.into(),
        name: pk(name),
        property_type: ty,
        nullable: true,
        default_value: None,
        description: LocalizedText::from(desc),
        classification: None,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// E-commerce ontology fixture
// ---------------------------------------------------------------------------

/// Standard e-commerce ontology for evaluation.
///
/// Nodes: Customer, Order, Product, Category, Review
/// Edges: PLACED, CONTAINS, BELONGS_TO, WROTE, ABOUT
pub fn ecommerce_ontology() -> OntologyIR {
    OntologyIR::new(
        "eval-ecommerce".to_string(),
        "E-Commerce Ontology".to_string(),
        LocalizedText::new("Standard evaluation ontology for NL-to-QueryIR testing"),
        1,
        vec![
            // Customer
            NodeTypeDef {
                id: "node-customer".into(),
                label: gl("Customer"),
                description: LocalizedText::new("A registered customer in the platform"),
                properties: vec![
                    prop(
                        "p-cust-name",
                        "name",
                        PropertyType::String,
                        Some("Full name of the customer"),
                    ),
                    prop(
                        "p-cust-email",
                        "email",
                        PropertyType::String,
                        Some("Email address (unique)"),
                    ),
                    prop(
                        "p-cust-city",
                        "city",
                        PropertyType::String,
                        Some("City of residence"),
                    ),
                ],
                constraints: vec![ConstraintDef {
                    id: "cst-cust-email".into(),
                    constraint: NodeConstraint::Unique {
                        property_ids: vec!["p-cust-email".into()],
                    },
                }],
                ..Default::default()
            },
            // Order
            NodeTypeDef {
                id: "node-order".into(),
                label: gl("Order"),
                description: LocalizedText::new("A purchase order placed by a customer"),
                properties: vec![
                    prop(
                        "p-ord-date",
                        "date",
                        PropertyType::Date,
                        Some("Date the order was placed"),
                    ),
                    prop(
                        "p-ord-total",
                        "total",
                        PropertyType::Float,
                        Some("Total order amount in USD"),
                    ),
                    prop(
                        "p-ord-status",
                        "status",
                        PropertyType::String,
                        Some("Order status: pending, confirmed, shipped, delivered, cancelled"),
                    ),
                ],
                constraints: vec![],
                ..Default::default()
            },
            // Product
            NodeTypeDef {
                id: "node-product".into(),
                label: gl("Product"),
                description: LocalizedText::new("A product available for purchase"),
                properties: vec![
                    prop(
                        "p-prod-name",
                        "name",
                        PropertyType::String,
                        Some("Product name"),
                    ),
                    prop(
                        "p-prod-price",
                        "price",
                        PropertyType::Float,
                        Some("Unit price in USD"),
                    ),
                    prop(
                        "p-prod-sku",
                        "sku",
                        PropertyType::String,
                        Some("Stock keeping unit (unique)"),
                    ),
                ],
                constraints: vec![ConstraintDef {
                    id: "cst-prod-sku".into(),
                    constraint: NodeConstraint::Unique {
                        property_ids: vec!["p-prod-sku".into()],
                    },
                }],
                ..Default::default()
            },
            // Category
            NodeTypeDef {
                id: "node-category".into(),
                label: gl("Category"),
                description: LocalizedText::new("Product category for classification"),
                properties: vec![prop(
                    "p-cat-name",
                    "name",
                    PropertyType::String,
                    Some("Category name (e.g., Electronics, Clothing, Books)"),
                )],
                constraints: vec![ConstraintDef {
                    id: "cst-cat-name".into(),
                    constraint: NodeConstraint::Unique {
                        property_ids: vec!["p-cat-name".into()],
                    },
                }],
                ..Default::default()
            },
            // Review
            NodeTypeDef {
                id: "node-review".into(),
                label: gl("Review"),
                description: LocalizedText::new("A product review written by a customer"),
                properties: vec![
                    prop(
                        "p-rev-rating",
                        "rating",
                        PropertyType::Int,
                        Some("Rating from 1 to 5"),
                    ),
                    nullable_prop(
                        "p-rev-text",
                        "text",
                        PropertyType::String,
                        Some("Review text content"),
                    ),
                    prop(
                        "p-rev-date",
                        "date",
                        PropertyType::Date,
                        Some("Date the review was written"),
                    ),
                ],
                constraints: vec![],
                ..Default::default()
            },
        ],
        vec![
            // PLACED: Customer → Order
            EdgeTypeDef {
                id: "edge-placed".into(),
                label: gl("PLACED"),
                description: LocalizedText::new("Customer placed an order"),
                source_node_id: "node-customer".into(),
                target_node_id: "node-order".into(),
                properties: vec![],
                cardinality: Cardinality::OneToMany,
                ..Default::default()
            },
            // CONTAINS: Order → Product
            EdgeTypeDef {
                id: "edge-contains".into(),
                label: gl("CONTAINS"),
                description: LocalizedText::new(
                    "Order contains a product. Customer→Product path: (Customer)-[:PLACED]->(Order)-[:CONTAINS]->(Product)",
                ),
                source_node_id: "node-order".into(),
                target_node_id: "node-product".into(),
                properties: vec![prop(
                    "p-cont-quantity",
                    "quantity",
                    PropertyType::Int,
                    Some("Number of units ordered"),
                )],
                cardinality: Cardinality::ManyToMany,
                ..Default::default()
            },
            // BELONGS_TO: Product → Category
            EdgeTypeDef {
                id: "edge-belongs-to".into(),
                label: gl("BELONGS_TO"),
                description: LocalizedText::new("Product belongs to a category"),
                source_node_id: "node-product".into(),
                target_node_id: "node-category".into(),
                properties: vec![],
                cardinality: Cardinality::ManyToOne,
                ..Default::default()
            },
            // WROTE: Customer → Review
            EdgeTypeDef {
                id: "edge-wrote".into(),
                label: gl("WROTE"),
                description: LocalizedText::new("Customer wrote a review"),
                source_node_id: "node-customer".into(),
                target_node_id: "node-review".into(),
                properties: vec![],
                cardinality: Cardinality::OneToMany,
                ..Default::default()
            },
            // ABOUT: Review → Product
            EdgeTypeDef {
                id: "edge-about".into(),
                label: gl("ABOUT"),
                description: LocalizedText::new("Review is about a product"),
                source_node_id: "node-review".into(),
                target_node_id: "node-product".into(),
                properties: vec![],
                cardinality: Cardinality::ManyToOne,
                ..Default::default()
            },
        ],
        vec![
            IndexDef::Single {
                id: "idx-cust-email".to_string(),
                node_id: "node-customer".into(),
                property_id: "p-cust-email".into(),
            },
            IndexDef::Single {
                id: "idx-prod-sku".to_string(),
                node_id: "node-product".into(),
                property_id: "p-prod-sku".into(),
            },
        ],
    )
}

// ---------------------------------------------------------------------------
// Evaluation cases
// ---------------------------------------------------------------------------

/// Standard set of 20 evaluation cases for the e-commerce ontology.
pub fn ecommerce_eval_cases() -> Vec<EvalCase> {
    let ontology = ecommerce_ontology();

    vec![
        // --- SimpleMatch ---
        EvalCase {
            id: "SM-01".into(),
            category: EvalCategory::SimpleMatch,
            question: "Find all customers".into(),
            ontology: ontology.clone(),
            expected_op: ExpectedOp::Match,
            expected_node_labels: vec![gl("Customer")],
            expected_edge_labels: vec![],
            description: "Simple node listing with default limit".into(),
        },
        EvalCase {
            id: "SM-02".into(),
            category: EvalCategory::SimpleMatch,
            question: "Show customers in Seoul".into(),
            ontology: ontology.clone(),
            expected_op: ExpectedOp::Match,
            expected_node_labels: vec![gl("Customer")],
            expected_edge_labels: vec![],
            description: "Single node with equality filter on city property".into(),
        },
        EvalCase {
            id: "SM-03".into(),
            category: EvalCategory::SimpleMatch,
            question: "List all product categories".into(),
            ontology: ontology.clone(),
            expected_op: ExpectedOp::Match,
            expected_node_labels: vec![gl("Category")],
            expected_edge_labels: vec![],
            description: "Simple listing of Category nodes".into(),
        },
        // --- RelationshipTraversal ---
        EvalCase {
            id: "RT-01".into(),
            category: EvalCategory::RelationshipTraversal,
            question: "What orders has customer John placed?".into(),
            ontology: ontology.clone(),
            expected_op: ExpectedOp::Match,
            expected_node_labels: vec![gl("Customer"), gl("Order")],
            expected_edge_labels: vec![gl("PLACED")],
            description: "One-hop traversal: Customer -PLACED-> Order with name filter".into(),
        },
        EvalCase {
            id: "RT-02".into(),
            category: EvalCategory::RelationshipTraversal,
            question: "Products in the Electronics category".into(),
            ontology: ontology.clone(),
            expected_op: ExpectedOp::Match,
            expected_node_labels: vec![gl("Product"), gl("Category")],
            expected_edge_labels: vec![gl("BELONGS_TO")],
            description: "One-hop traversal: Product -BELONGS_TO-> Category with filter".into(),
        },
        EvalCase {
            id: "RT-03".into(),
            category: EvalCategory::RelationshipTraversal,
            question: "Customers who wrote reviews with rating 5".into(),
            ontology: ontology.clone(),
            expected_op: ExpectedOp::Match,
            expected_node_labels: vec![gl("Customer"), gl("Review")],
            expected_edge_labels: vec![gl("WROTE")],
            description: "One-hop traversal with filter on the target node property".into(),
        },
        EvalCase {
            id: "RT-04".into(),
            category: EvalCategory::RelationshipTraversal,
            question: "What products did customer Alice order?".into(),
            ontology: ontology.clone(),
            expected_op: ExpectedOp::Match,
            expected_node_labels: vec![gl("Customer"), gl("Order"), gl("Product")],
            expected_edge_labels: vec![gl("PLACED"), gl("CONTAINS")],
            description: "Two-hop traversal: Customer -PLACED-> Order -CONTAINS-> Product".into(),
        },
        // --- Aggregation ---
        EvalCase {
            id: "AG-01".into(),
            category: EvalCategory::Aggregation,
            question: "How many orders per customer?".into(),
            ontology: ontology.clone(),
            expected_op: ExpectedOp::Match,
            expected_node_labels: vec![gl("Customer"), gl("Order")],
            expected_edge_labels: vec![gl("PLACED")],
            description: "Count aggregation grouped by customer".into(),
        },
        EvalCase {
            id: "AG-02".into(),
            category: EvalCategory::Aggregation,
            question: "Average order total by customer".into(),
            ontology: ontology.clone(),
            expected_op: ExpectedOp::Match,
            expected_node_labels: vec![gl("Customer"), gl("Order")],
            expected_edge_labels: vec![gl("PLACED")],
            description: "Average aggregation on order total grouped by customer".into(),
        },
        EvalCase {
            id: "AG-03".into(),
            category: EvalCategory::Aggregation,
            question: "How many reviews per product?".into(),
            ontology: ontology.clone(),
            expected_op: ExpectedOp::Match,
            expected_node_labels: vec![gl("Review"), gl("Product")],
            expected_edge_labels: vec![gl("ABOUT")],
            description: "Count aggregation grouped by product".into(),
        },
        EvalCase {
            id: "AG-04".into(),
            category: EvalCategory::Aggregation,
            question: "Total revenue by category".into(),
            ontology: ontology.clone(),
            expected_op: ExpectedOp::Match,
            expected_node_labels: vec![gl("Order"), gl("Product"), gl("Category")],
            expected_edge_labels: vec![gl("CONTAINS"), gl("BELONGS_TO")],
            description: "Sum aggregation across multi-hop traversal".into(),
        },
        // --- TopN ---
        EvalCase {
            id: "TN-01".into(),
            category: EvalCategory::TopN,
            question: "Top 5 most expensive products".into(),
            ontology: ontology.clone(),
            expected_op: ExpectedOp::Match,
            expected_node_labels: vec![gl("Product")],
            expected_edge_labels: vec![],
            description: "Top-N ordering by price DESC with limit 5".into(),
        },
        EvalCase {
            id: "TN-02".into(),
            category: EvalCategory::TopN,
            question: "Most recent 10 orders".into(),
            ontology: ontology.clone(),
            expected_op: ExpectedOp::Match,
            expected_node_labels: vec![gl("Order")],
            expected_edge_labels: vec![],
            description: "Top-N ordering by date DESC with limit 10".into(),
        },
        // --- MultiFilter ---
        EvalCase {
            id: "MF-01".into(),
            category: EvalCategory::MultiFilter,
            question: "Products over $100 with rating above 4".into(),
            ontology: ontology.clone(),
            expected_op: ExpectedOp::Match,
            expected_node_labels: vec![gl("Product"), gl("Review")],
            expected_edge_labels: vec![gl("ABOUT")],
            description: "Multiple filters: price > 100 AND rating > 4 across relationship".into(),
        },
        EvalCase {
            id: "MF-02".into(),
            category: EvalCategory::MultiFilter,
            question: "Delivered orders with total over 500".into(),
            ontology: ontology.clone(),
            expected_op: ExpectedOp::Match,
            expected_node_labels: vec![gl("Order")],
            expected_edge_labels: vec![],
            description: "Multiple filters on same node: status = delivered AND total > 500".into(),
        },
        // --- PathFinding ---
        EvalCase {
            id: "PF-01".into(),
            category: EvalCategory::PathFinding,
            question: "How are customer Alice and product Widget connected?".into(),
            ontology: ontology.clone(),
            expected_op: ExpectedOp::PathFind,
            expected_node_labels: vec![gl("Customer"), gl("Product")],
            expected_edge_labels: vec![],
            description: "Shortest path between Customer and Product".into(),
        },
        // --- MultiStep ---
        EvalCase {
            id: "MS-01".into(),
            category: EvalCategory::MultiStep,
            question: "Customers who ordered products in the Electronics category".into(),
            ontology: ontology.clone(),
            expected_op: ExpectedOp::Match,
            expected_node_labels: vec![
                gl("Customer"),
                gl("Order"),
                gl("Product"),
                gl("Category"),
            ],
            expected_edge_labels: vec![gl("PLACED"), gl("CONTAINS"), gl("BELONGS_TO")],
            description: "Multi-hop traversal through Order and Product to Category".into(),
        },
        EvalCase {
            id: "MS-02".into(),
            category: EvalCategory::MultiStep,
            question: "Number of unique products ordered per customer".into(),
            ontology: ontology.clone(),
            expected_op: ExpectedOp::Match,
            expected_node_labels: vec![gl("Customer"), gl("Order"), gl("Product")],
            expected_edge_labels: vec![gl("PLACED"), gl("CONTAINS")],
            description: "Count DISTINCT aggregation over multi-hop pattern".into(),
        },
        // --- EdgeCase ---
        EvalCase {
            id: "EC-01".into(),
            category: EvalCategory::EdgeCase,
            question: "Show me all warehouses".into(),
            ontology: ontology.clone(),
            expected_op: ExpectedOp::Match,
            expected_node_labels: vec![],
            expected_edge_labels: vec![],
            description: "Query about entity not in ontology — should handle gracefully".into(),
        },
        EvalCase {
            id: "EC-02".into(),
            category: EvalCategory::EdgeCase,
            question: "Find products with reviews".into(),
            ontology: ontology.clone(),
            expected_op: ExpectedOp::Match,
            expected_node_labels: vec![gl("Product"), gl("Review")],
            expected_edge_labels: vec![gl("ABOUT")],
            description: "Existence check: products that have at least one review".into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecommerce_ontology_is_valid() {
        let ont = ecommerce_ontology();
        let errors = ont.validate();
        assert!(errors.is_empty(), "Ontology validation errors: {errors:?}");
    }

    #[test]
    fn eval_cases_have_unique_ids() {
        let cases = ecommerce_eval_cases();
        let mut ids: Vec<&str> = cases.iter().map(|c| c.id.as_str()).collect();
        let total = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), total, "Duplicate eval case IDs found");
    }

    #[test]
    fn eval_cases_cover_all_categories() {
        let cases = ecommerce_eval_cases();
        let categories: std::collections::HashSet<EvalCategory> =
            cases.iter().map(|c| c.category).collect();

        assert!(categories.contains(&EvalCategory::SimpleMatch));
        assert!(categories.contains(&EvalCategory::RelationshipTraversal));
        assert!(categories.contains(&EvalCategory::Aggregation));
        assert!(categories.contains(&EvalCategory::PathFinding));
        assert!(categories.contains(&EvalCategory::MultiFilter));
        assert!(categories.contains(&EvalCategory::TopN));
        assert!(categories.contains(&EvalCategory::MultiStep));
        assert!(categories.contains(&EvalCategory::EdgeCase));
    }

    #[test]
    fn eval_cases_minimum_count() {
        let cases = ecommerce_eval_cases();
        assert!(
            cases.len() >= 15,
            "Expected at least 15 eval cases, got {}",
            cases.len()
        );
    }
}
